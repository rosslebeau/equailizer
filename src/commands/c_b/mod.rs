mod create_updates;
mod process_tags;

use crate::commands::c_b::create_updates::create_updates;
use crate::commands::c_b::process_tags::process_tags;
use crate::config::{self, *};
use crate::email::{self, Txn};
use crate::lunch_money::api::update_transaction::TransactionUpdate;
use crate::lunch_money::model::transaction;
use crate::persist::Batch;
use crate::usd::USD;
use crate::{
    lunch_money, lunch_money::api::Client, lunch_money::api::update_transaction,
    lunch_money::api::update_transaction::SplitUpdateItem,
    lunch_money::model::transaction::TransactionId, lunch_money::model::transaction::*, persist,
};
use chrono::NaiveDate;
use rand::random_bool;
use rust_decimal::prelude::*;
use uuid::{self, Uuid};

pub async fn create_batch(
    start_date: NaiveDate,
    end_date: NaiveDate,
    profile: &String,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    // get transactions from creditor
    if start_date.cmp(&end_date) == std::cmp::Ordering::Greater {
        return Err("start date cannot be after end date".into());
    }

    // let mut batch_total: USD = USD::new(dec!(0));

    let creditor_client = Client {
        auth_token: config.creditor.api_key.to_owned(),
    };
    let txns = creditor_client
        .get_transactions(start_date, end_date)
        .await?;

    // process tags on retrieved txns
    let processed = process_tags(
        txns,
        &config::TAG_BATCH_ADD.to_string(),
        &config::TAG_BATCH_SPLIT.to_string(),
    );

    // create Actions for the processed results
    let (add_updates, split_updates) = create_updates(processed, config.creditor.proxy_category_id);

    // execute the Actions
    for update in add_updates {
        creditor_client.update_txn2(0, update).await?;
    }
    for (txn_update, split_update) in split_updates {
        let debtor_split_id = creditor_client
            .update_txn_and_split2(0, txn_update, split_update)
            .await?
            .split_ids
            .get(1)
            .ok_or("no item in position 1 of split ids in transaction update response")?;
    }

    // Save batch to local data
    let batch_id = Uuid::new_v4().to_string();

    // Need to sum added txns, and get debtor split amounts and sum those too
    let batch_total: USD = USD::new_from_cents(0);

    // Need to get split ids back from split actions
    let batch_txn_ids = vec![];

    let batch: Batch = Batch {
        id: batch_id.clone(),
        amount: batch_total,
        transaction_ids: batch_txn_ids,
        reconciliation: None,
    };

    persist::save_batch(&batch, profile);

    // configure/send email
    let batch_txns_for_email: Vec<email::Txn> = vec![];
    email::send_email(&batch_id, &batch_total, batch_txns_for_email, config).await?;

    return Ok(());
}
