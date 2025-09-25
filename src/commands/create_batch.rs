use crate::config::{self, *};
use crate::email;
use crate::usd::USD;
use crate::{
    lunch_money, lunch_money::api::update_transaction, lunch_money::api::update_transaction::Split,
    lunch_money::model::transaction::*, persist,
};
use chrono::NaiveDate;
use rand::random_bool;
use rust_decimal::prelude::*;
use uuid::{self, Uuid};

pub struct SuccessResult {
    pub batch_label: String,
    pub batch_amount: USD,
}

pub async fn run(
    start_date: NaiveDate,
    end_date: NaiveDate,
    config: &Config,
) -> Result<SuccessResult, Box<dyn std::error::Error>> {
    if start_date.cmp(&end_date) == std::cmp::Ordering::Greater {
        return Err("start date cannot be after end date".into());
    }

    let batch_id = Uuid::new_v4();
    let batch_label: String = eq_batch_name(batch_id);
    let mut batch_total: USD = USD::new(dec!(0));

    let lm_creditor_client = crate::lunch_money::api::Client {
        auth_token: config.creditor.api_key.to_owned(),
    };
    let txns = lm_creditor_client
        .get_transactions(start_date, end_date)
        .await?;

    for txn in txns {
        let tag_names: Vec<String> = txn.tags.iter().map(|t| t.name.to_owned()).collect();
        if tag_names.contains(&config::TAG_BATCH_SPLIT.into()) {
            let splits = create_random_even_splits(&txn, &batch_label, config);

            batch_total = batch_total + splits.debtor_split.amount;

            let txn_update = update_transaction::TransactionUpdate {
                payee: None,
                category_id: None,
                notes: None,
                tags: Some(tags_by_removing_tag(&txn, config::TAG_BATCH_SPLIT.into())),
                status: Some(TransactionStatus::Cleared),
            };

            lm_creditor_client
                .update_txn_and_split(
                    txn.id,
                    &txn_update,
                    &vec![splits.creditor_split, splits.debtor_split],
                )
                .await?;
        } else if tag_names.contains(&config::TAG_BATCH_ADD.into()) {
            batch_total = batch_total + txn.amount;

            let txn_update = update_transaction::TransactionUpdate {
                payee: None,
                category_id: Some(config.creditor.proxy_category_id),
                notes: Some(batch_label.to_owned()),
                tags: Some(tags_by_removing_tag(&txn, config::TAG_BATCH_ADD.into())),
                status: Some(TransactionStatus::Cleared),
            };

            lm_creditor_client
                .update_txn_only(txn.id, &txn_update)
                .await?;
        }
    }

    persist::save_new_batch_metadata(&batch_label, start_date, end_date)?;

    email::send_email(&batch_label, &batch_total, config).await?;

    let result = SuccessResult {
        batch_label: batch_label,
        batch_amount: batch_total,
    };
    return Ok(result);
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
