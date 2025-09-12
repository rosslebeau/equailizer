use crate::config::{self, *};
use crate::usd::USD;
use crate::{
    lunch_money, lunch_money::api::update_transaction, lunch_money::api::update_transaction::Split,
    lunch_money::model::transaction::*,
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
    config: &Config,
    start_date: NaiveDate,
    end_date: NaiveDate,
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
            let (creditor_amt, debtor_amt) = random_even_split(txn.amount);

            let splits =
                create_splits_for_batch(creditor_amt, debtor_amt, batch_label.to_owned(), config);
            let txn_update = update_transaction::TransactionUpdate {
                payee: None,
                notes: None,
                tags: Some(tags_by_removing_tag(&txn, config::TAG_BATCH_SPLIT.into())),
                status: Some(TransactionStatus::Cleared),
            };

            lm_creditor_client
                .update_txn_and_split(txn.id, txn_update, splits)
                .await?;
            batch_total = batch_total + debtor_amt;
        } else if tag_names.contains(&config::TAG_BATCH_ADD.into()) {
            let txn_update = update_transaction::TransactionUpdate {
                payee: None,
                notes: Some(batch_label.to_owned()),
                tags: Some(tags_by_removing_tag(&txn, config::TAG_BATCH_ADD.into())),
                status: Some(TransactionStatus::Cleared),
            };

            lm_creditor_client
                .update_txn_only(txn.id, txn_update)
                .await?;
            batch_total = batch_total + txn.amount;
        }
    }

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

fn create_splits_for_batch(
    creditor_amt: USD,
    debtor_amt: USD,
    batch_label: String,
    config: &Config,
) -> Vec<Split> {
    let creditor_split = lunch_money::api::update_transaction::Split {
        amount: creditor_amt,
        payee: None,
        category_id: None,
        notes: None,
    };
    let debtor_split = lunch_money::api::update_transaction::Split {
        amount: debtor_amt,
        payee: None,
        category_id: Some(config.creditor.proxy_category_id),
        notes: Some(batch_label),
    };
    return vec![creditor_split, debtor_split];
}

fn tags_by_removing_tag(txn: &Transaction, tag_to_remove: String) -> Vec<String> {
    txn.tags
        .iter()
        .filter(|t| (**t).name != tag_to_remove)
        .map(|t| t.name.to_owned())
        .collect()
}
