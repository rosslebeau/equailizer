mod create_updates;
mod process_tags;

use std::collections::HashMap;

use crate::commands::create_batch::create_updates::create_updates;
use crate::commands::create_batch::process_tags::{Issue, process_tags};
use crate::config::{self, *};
use crate::email::{self, Txn};
use crate::lunch_money::api::update_transaction::TransactionUpdateItem;
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

    if processed.txns_to_add.iter().count() + processed.txns_to_split.iter().count() == 0 {
        tracing::info!("No valid transactions found to create batch from");
        return Ok(());
    }

    // We can create the models used for the email now,
    // but only for transactions we're going to add, not split.
    // (We don't know the ids or amounts of the splits yet).
    let mut added_txns_for_email: Vec<Txn> = processed
        .txns_to_add
        .iter()
        .map(|t| Txn {
            payee: t.payee.clone(),
            amount: t.amount,
            date: t.date,
        })
        .collect();

    // Collect the payees of all the transactions we are going to split
    // so that we can display them in the email later, once we get the
    // ids and amounts of the splits.
    let split_txns_payee_and_date_by_id: HashMap<TransactionId, (String, NaiveDate)> = processed
        .txns_to_split
        .iter()
        .fold(HashMap::new(), |mut acc, t| {
            acc.insert(t.id, (t.payee.clone(), t.date));
            acc
        });

    // Create actionable updates for the processed results.
    let (add_updates, split_updates) =
        create_updates(&processed, config.creditor.proxy_category_id);

    // Execute the updates. For updates with splits, extract the amount for the
    // debtor's side (2nd item), and then save the id for that item from the Lunch Money
    // API response.
    let mut added_batch_txn_ids: Vec<TransactionId> = vec![];

    for update in add_updates {
        added_batch_txn_ids.push(update.0);
        creditor_client.update_transaction(update).await?;
    }

    let (mut split_ids, mut split_txns_for_email) = {
        let mut post_split_txn_ids: Vec<TransactionId> = vec![];
        let mut txns_for_email: Vec<Txn> = vec![];

        for split_update in split_updates {
            let split_update_item = split_update
                .2
                .get(1)
                .expect("split update contained fewer than 2 split items");

            txns_for_email.push(Txn {
                payee: split_txns_payee_and_date_by_id
                    .get(&split_update.0)
                    .expect("unexpected transaction id mismatch while splitting")
                    .0
                    .clone(),
                amount: split_update_item.amount,
                date: split_txns_payee_and_date_by_id
                    .get(&split_update.0)
                    .expect("unexpected transaction id mismatch while splitting")
                    .1,
            });

            let debtor_split_id = *creditor_client
                .update_transaction_and_split(split_update)
                .await?
                .split_ids
                .get(1)
                .ok_or("no item in position 1 of split ids in transaction update response - expected debtor proxy split id")?;
            post_split_txn_ids.push(debtor_split_id);
        }

        (post_split_txn_ids, txns_for_email)
    };

    // Clean up the ids and email txn models now that we have all the info we need.
    let batch_txn_ids = {
        added_batch_txn_ids.append(&mut split_ids);
        added_batch_txn_ids
    };
    drop(split_ids);

    let email_txns = {
        added_txns_for_email.append(&mut split_txns_for_email);
        added_txns_for_email
    };
    drop(split_txns_for_email);

    // We can use the Txns we just made for the email to get the total amount of the batch.
    let batch_total_amount: USD = email_txns
        .iter()
        .map(|t| t.amount)
        .reduce(|acc, t| acc + t)
        .expect("no items in email_txns");

    let batch_id = Uuid::new_v4().to_string();

    // Save batch to local data.
    let batch: Batch = Batch {
        id: Uuid::new_v4().to_string(),
        amount: batch_total_amount,
        transaction_ids: batch_txn_ids,
        reconciliation: None,
    };
    persist::save_batch(&batch, profile)?;

    // configure/send email
    // let issues = processed.issues;
    let email_warnings: Vec<String> = processed.issues.iter().map(|i| text_for_issue(i)).collect();
    email::send_email(
        &batch_id,
        &batch_total_amount,
        email_txns,
        email_warnings,
        config,
    )
    .await?;

    return Ok(());
}

fn text_for_issue(issue: &Issue) -> String {
    match issue {
        Issue::AddTagHasChildren(txn) => format!(
            "Transaction was tagged for batch, but it has children: {}",
            txn
        ),
        Issue::SplitTagHasParent(txn) => format!(
            "Transaction was tagged to split, but it has a parent: {}",
            txn
        ),
        Issue::SplitTagHasChildren(txn) => format!(
            "Transaction was tagged to split, but it already has children: {}",
            txn
        ),
    }
}
