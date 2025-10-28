use crate::config::{self, *};
use crate::email;
use crate::persist::Batch;
use crate::usd::USD;
use crate::{
    lunch_money, lunch_money::api::Client, lunch_money::api::update_transaction,
    lunch_money::api::update_transaction::Split,
    lunch_money::model::transaction::Id as TransactionId, lunch_money::model::transaction::*,
    persist,
};
use chrono::NaiveDate;
use rand::random_bool;
use rust_decimal::prelude::*;
use uuid::{self, Uuid};

pub async fn run(
    start_date: NaiveDate,
    end_date: NaiveDate,
    config: &Config,
    profile: &String,
) -> Result<(), Box<dyn std::error::Error>> {
    let span = tracing::info_span!("Create Batch");
    let _enter = span.enter();
    tracing::debug!("Starting create batch");

    if start_date.cmp(&end_date) == std::cmp::Ordering::Greater {
        return Err("start date cannot be after end date".into());
    }

    let batch_id = Uuid::new_v4();
    let batch_label: String = eq_batch_name(batch_id);
    let mut batch_total: USD = USD::new(dec!(0));

    let lm_creditor_client = Client {
        auth_token: config.creditor.api_key.to_owned(),
    };
    let txns = lm_creditor_client
        .get_transactions(start_date, end_date)
        .await?;

    tracing::debug!(?txns, "received transactions");

    let mut found_valid_txns = false;
    let mut earliest_txn_date = end_date;
    let mut batch_txn_ids: Vec<TransactionId> = Vec::new();
    for txn in txns {
        let tag_names: Vec<String> = txn.tags.iter().map(|t| t.name.to_owned()).collect();
        if tag_names.contains(&config::TAG_BATCH_SPLIT.into()) {
            tracing::debug!(txn.id, "found split tag");
            if txn.has_children {
                tracing::debug!(
                    txn.id,
                    "transaction has children. invalid target for split. removing split tag"
                );

                remove_tag(&txn, config::TAG_BATCH_SPLIT.into(), &lm_creditor_client).await?;
            } else {
                found_valid_txns = true;

                if txn.date < earliest_txn_date {
                    earliest_txn_date = txn.date;
                }

                let (split_id, split_amt) =
                    split_txn(&txn, &batch_label, &lm_creditor_client, config).await?;
                batch_txn_ids.push(split_id);
                batch_total = batch_total + split_amt;
            }
        } else if tag_names.contains(&config::TAG_BATCH_ADD.into()) {
            tracing::debug!(txn.id, "found batch tag");

            if txn.has_children {
                tracing::debug!(
                    txn.id,
                    "transaction has children. invalid target for batching. removing split tag"
                );

                remove_tag(&txn, config::TAG_BATCH_SPLIT.into(), &lm_creditor_client).await?;
            } else {
                found_valid_txns = true;

                if txn.date < earliest_txn_date {
                    earliest_txn_date = txn.date;
                }

                batch_total = batch_total + txn.amount;

                add_txn_to_batch(&txn, &lm_creditor_client, config).await?;
                batch_txn_ids.push(txn.id);
            }
        }
    }

    if !found_valid_txns {
        return Err("no valid transactions with batching tag found".into());
    }

    persist::save_batch(
        &Batch {
            name: batch_label.to_owned(),
            start_date: earliest_txn_date,
            end_date: end_date,
            amount: batch_total,
            transaction_ids: batch_txn_ids,
            reconciliation: None,
        },
        profile,
    )?;

    email::send_email(&batch_label, &batch_total, config).await?;

    tracing::info!(batch_label, %batch_total, "Created batch successfully");

    return Ok(());
}

// Returns the debtor's split id and amount, because that's all we care about for now
async fn split_txn(
    txn: &Transaction,
    batch_label: &String,
    client: &Client,
    config: &Config,
) -> Result<(TransactionId, USD), Box<dyn std::error::Error>> {
    tracing::debug!(txn.id, "splitting transaction");

    let splits = create_random_even_splits(&txn, &batch_label, config);
    let debtor_split_amt = splits.debtor_split.amount;

    let txn_update = update_transaction::TransactionUpdate {
        payee: None,
        category_id: None,
        notes: None,
        tags: Some(tags_by_removing_tag(&txn, config::TAG_BATCH_SPLIT.into())),
        status: Some(TransactionStatus::Cleared),
    };

    let debtor_split_id = client
        .update_txn_and_split(
            txn.id,
            &txn_update,
            &vec![splits.creditor_split, splits.debtor_split],
        )
        .await?
        .splits
        .ok_or("No split ids in split txn response")?
        .get(1)
        .expect("fewer than 2 split ids in split txn response")
        .to_owned();

    return Ok((debtor_split_id, debtor_split_amt));
}

async fn remove_tag(
    txn: &Transaction,
    tag_to_remove: String,
    client: &Client,
) -> Result<(), Box<dyn std::error::Error>> {
    let txn_update = update_transaction::TransactionUpdate {
        payee: None,
        category_id: None,
        notes: None,
        tags: Some(tags_by_removing_tag(&txn, tag_to_remove)),
        status: None,
    };

    client.update_txn_only(txn.id, &txn_update).await?;
    return Ok(());
}

async fn add_txn_to_batch(
    txn: &Transaction,
    client: &Client,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let txn_update = update_transaction::TransactionUpdate {
        payee: None,
        category_id: Some(config.creditor.proxy_category_id),
        notes: None,
        tags: Some(tags_by_removing_tag(&txn, config::TAG_BATCH_ADD.into())),
        status: Some(TransactionStatus::Cleared),
    };

    client.update_txn_only(txn.id, &txn_update).await?;

    return Ok(());
}

// Always round one up and one down, so the total stays the same
// Return the split values in a random order, so over time neither half is larger
fn random_even_split(amt: USD) -> (USD, USD) {
    let half1 = (amt.value() / dec!(2))
        .round_dp_with_strategy(2, rust_decimal::RoundingStrategy::AwayFromZero);
    let half2 =
        (amt.value() / dec!(2)).round_dp_with_strategy(2, rust_decimal::RoundingStrategy::ToZero);
    assert_eq!(
        amt.value(),
        (half1 + half2),
        "rounded splits not equal to starting total"
    );

    if random_bool(0.5) {
        return (USD::new(half1), USD::new(half2));
    } else {
        return (USD::new(half2), USD::new(half1));
    }
}

struct EvenSplit {
    creditor_split: Split,
    debtor_split: Split,
}

fn create_random_even_splits(
    txn: &Transaction,
    batch_label: &String,
    config: &Config,
) -> EvenSplit {
    let (creditor_amt, debtor_amt) = random_even_split(txn.amount);

    let creditor_split = lunch_money::api::update_transaction::Split {
        amount: creditor_amt,
        payee: None,
        category_id: txn.category_id,
        notes: None,
        date: None,
    };
    let debtor_split = lunch_money::api::update_transaction::Split {
        amount: debtor_amt,
        payee: None,
        category_id: Some(config.creditor.proxy_category_id),
        notes: Some(batch_label.to_owned()),
        date: None,
    };
    return EvenSplit {
        creditor_split: creditor_split,
        debtor_split: debtor_split,
    };
}

fn tags_by_removing_tag(txn: &Transaction, tag_to_remove: String) -> Vec<String> {
    txn.tags
        .iter()
        .filter(|t| (**t).name != tag_to_remove)
        .map(|t| t.name.to_owned())
        .collect()
}
