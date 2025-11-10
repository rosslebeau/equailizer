mod create_updates;
mod process_tags;

use std::collections::HashMap;

use crate::commands::create_batch::create_updates::create_updates;
use crate::commands::create_batch::process_tags::process_tags;
use crate::config::{self, *};
use crate::email::{self, Txn};
use crate::lunch_money::api::update_transaction::{
    TransactionAndSplitUpdate, TransactionUpdate, TransactionUpdateItem,
};
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

#[derive(Debug, PartialEq)]
pub enum Issue {
    AddTagHasChildren(TransactionId),
    SplitTagHasParent(TransactionId),
    SplitTagHasChildren(TransactionId),
    TransactionUpdateError(TransactionId, String),
}

pub async fn create_batch(
    start_date: NaiveDate,
    end_date: NaiveDate,
    profile: &String,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let span = tracing::info_span!("Create Batch");
    let _enter = span.enter();
    tracing::debug!("Starting");

    if start_date.cmp(&end_date) == std::cmp::Ordering::Greater {
        return Err("start date cannot be after end date".into());
    }

    // Get all transactions in provided date range.
    let creditor_client = Client {
        auth_token: config.creditor.api_key.to_owned(),
    };
    let txns = creditor_client
        .get_transactions(start_date, end_date)
        .await?;

    // Process tags on the retrieved transactions to see what should be
    // added to the batch.
    let processed = process_tags(
        txns,
        &config::TAG_BATCH_ADD.to_string(),
        &config::TAG_BATCH_SPLIT.to_string(),
    );

    // Check that we found at least 1 valid transaction.
    if processed.txns_to_add.iter().count() + processed.txns_to_split.iter().count() == 0 {
        tracing::info!("No valid transactions found to create batch from");
        return Ok(());
    }

    // Create actionable updates for the processed results.
    let (add_updates, split_updates) = create_updates(processed, config.creditor.proxy_category_id);

    // Prepare final output data.
    let mut batched_txn_info: Vec<(TransactionId, Txn)> = vec![];
    let mut issues: Vec<Issue> = vec![];

    // Execute adds and append results to output.
    {
        let (mut added_ids_and_email_txns, mut added_issues) =
            execute_adds(add_updates, &creditor_client).await;
        batched_txn_info.append(&mut added_ids_and_email_txns);
        issues.append(&mut added_issues);
    }

    // Execute splits and append results to output.
    {
        let (mut added_ids_and_email_txns, mut added_issues) =
            execute_splits(split_updates, &creditor_client).await;
        batched_txn_info.append(&mut added_ids_and_email_txns);
        issues.append(&mut added_issues);
    }

    // Scoop up all the data from the batched transactions into the relevant formats for output
    let (batched_ids, email_txns, total_amount): (Vec<TransactionId>, Vec<Txn>, USD) =
        batched_txn_info.into_iter().fold(
            (vec![], vec![], USD::new_from_cents(0)),
            |(mut ids, mut txns, amt), x| {
                let tot = amt + x.1.amount;
                ids.push(x.0);
                txns.push(x.1);
                return (ids, txns, tot);
            },
        );

    // Create batch id and save to local data.
    let batch_id = Uuid::new_v4().to_string();
    tracing::debug!(batch_id, "Saving new batch");
    let batch: Batch = Batch {
        id: Uuid::new_v4().to_string(),
        amount: total_amount,
        transaction_ids: batched_ids,
        reconciliation: None,
    };
    persist::save_batch(&batch, profile)?;

    // Send the batch notification email.
    let email_warnings: Vec<String> = issues.iter().map(|i| text_for_issue(i)).collect();
    email::send_email(&batch_id, &total_amount, email_txns, email_warnings, config).await?;

    tracing::debug!(amount = ?total_amount, batch_id, "Finished creating batch");
    Ok(())
}

// Execute adding these transactions to the batch with their associated pre-prepared update.
// Return info about the added transactions and any issues encountered during the operation.
async fn execute_adds(
    txns_and_updates: Vec<(Transaction, TransactionUpdate)>,
    client: &Client,
) -> (Vec<(TransactionId, Txn)>, Vec<Issue>) {
    let mut batched_txn_info: Vec<(TransactionId, Txn)> = vec![];
    let mut issues: Vec<Issue> = vec![];

    for (txn, update) in txns_and_updates {
        let result = client.update_transaction(update).await;
        match result {
            Ok(_) => {
                batched_txn_info.push((
                    txn.id,
                    Txn {
                        payee: txn.payee,
                        amount: txn.amount,
                        date: txn.date,
                    },
                ));
            }
            Err(e) => {
                issues.push(Issue::TransactionUpdateError(txn.id, e.to_string()));
            }
        }
    }

    return (batched_txn_info, issues);
}

// Execute splitting and adding these transactions to the batch with their associated pre-prepared update.
// Return info about the added transactions and any issues encountered during the operation.
// Returns info about the transactions added to the batch - i.e. after splitting,
// return the debtor's split txn info
async fn execute_splits(
    txns_and_updates: Vec<(Transaction, TransactionAndSplitUpdate)>,
    client: &Client,
) -> (Vec<(TransactionId, Txn)>, Vec<Issue>) {
    let mut batched_txn_info: Vec<(TransactionId, Txn)> = vec![];
    let mut issues: Vec<Issue> = vec![];

    for (txn, update) in txns_and_updates {
        // Grab amount out of update before consuming it during execution
        let split_amount = update
            .2
            .get(1)
            .expect("split update contained fewer than 2 split items")
            .amount;

        let result = client.update_transaction_and_split(update).await;

        match result {
            Ok(split_response) => {
                match split_response.split_ids.get(1).ok_or("no item in position 1 of split ids in transaction update response - expected debtor proxy split id") {
                    Ok(batched_id) => {
                        batched_txn_info.push((
                            *batched_id,
                            Txn {
                                payee: txn.payee,
                                amount: split_amount,
                                date: txn.date,
                            },
                        ));
                    }
                    Err(e) => {
                        issues.push(Issue::TransactionUpdateError(txn.id, e.to_string()));
                    }
                }
            }
            Err(e) => {
                issues.push(Issue::TransactionUpdateError(txn.id, e.to_string()));
            }
        }
    }

    return (batched_txn_info, issues);
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
        Issue::TransactionUpdateError(txn, e_str) => {
            format!("Error when updating transaction {}: {}", txn, e_str)
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    #[test]
    fn test_execute_adds() {
        // Need a way to inject a test Client to test execute_adds
    }

    #[test]
    fn test_execute_splits() {
        // Need a way to inject a test Client to test execute_splits
    }
}
