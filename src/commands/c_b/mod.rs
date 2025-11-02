mod create_actions;
mod process_tags;

use crate::commands::c_b::create_actions::create_actions;
use crate::commands::c_b::process_tags::process_tags;
use crate::config::{self, *};
use crate::email;
use crate::lunch_money::api::update_transaction::{Action, TransactionUpdate};
use crate::lunch_money::model::transaction;
use crate::persist::Batch;
use crate::usd::USD;
use crate::{
    lunch_money, lunch_money::api::Client, lunch_money::api::update_transaction,
    lunch_money::api::update_transaction::Split, lunch_money::model::transaction::TransactionId,
    lunch_money::model::transaction::*, persist,
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

    let batch_id = Uuid::new_v4().to_string();
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
    let actions = create_actions(processed, config.creditor.proxy_category_id);

    // execute the Actions
    for action in actions {
        creditor_client.update2(0, action).await?;
    }

    // Save batch to local data

    // configure/send email
    // Some(tags_by_removing_tag(&txn, config::TAG_BATCH_ADD.into()))

    return Ok(());
}
