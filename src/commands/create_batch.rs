mod create_updates;
mod process_tags;

use crate::commands::create_batch::create_updates::{create_resplit_items, create_updates};
use crate::commands::create_batch::process_tags::process_tags;
use crate::config;
use crate::email::{BatchNotifier, Txn};
use crate::error::{Error, Result};
use crate::issue::Issue;
use crate::lunch_money::api::update_transaction::{
    TransactionAndSplitUpdate, TransactionUpdate,
};
use crate::lunch_money::api::LunchMoney;
use crate::lunch_money::model::transaction::{Transaction, TransactionId};
use crate::persist::{Batch, Persistence};
use crate::plugin::PluginManager;
use crate::usd::USD;
use chrono::NaiveDate;
use std::collections::HashMap;
use uuid::Uuid;

pub async fn create_batch(
    start_date: NaiveDate,
    end_date: NaiveDate,
    config: &config::Config,
    api: &(impl LunchMoney + Sync),
    persistence: &(impl Persistence + Sync),
    notifier: &(impl BatchNotifier + Sync),
    plugins: &mut PluginManager,
) -> Result<()> {
    let span = tracing::info_span!("Create Batch");
    let _enter = span.enter();

    if start_date.cmp(&end_date) == std::cmp::Ordering::Greater {
        return Err(Error::InvalidDateRange);
    }

    tracing::info!(
        start_date = %start_date.format("%Y-%m-%d"),
        end_date = %end_date.format("%Y-%m-%d"),
        "Fetching transactions"
    );

    // Get all transactions in provided date range.
    let txns = api.get_transactions(start_date, end_date).await?;
    let all_txns = txns.clone(); // Keep for sibling lookup during resplits

    // Process tags on the retrieved transactions to see what should be
    // added to the batch.
    let mut processed = process_tags(
        txns,
        &config::TAG_BATCH_ADD.to_string(),
        &config::TAG_BATCH_SPLIT.to_string(),
    );

    let add_count = processed.txns_to_add.len();
    let split_count = processed.txns_to_split.len();
    let resplit_count = processed.txns_to_resplit.len();

    // Check that we found at least 1 valid transaction.
    if add_count + split_count + resplit_count == 0 {
        tracing::info!("No tagged transactions found — nothing to batch");
        return Ok(());
    }

    tracing::info!(
        to_add = add_count,
        to_split = split_count,
        to_resplit = resplit_count,
        "Tagged transactions found"
    );

    // Capture tag-processing issues before consuming the processed data.
    let mut issues: Vec<Issue> = processed.issues.drain(..).collect();
    for issue in &issues {
        tracing::warn!("{}", issue);
    }

    // Extract resplit transactions before create_updates consumes the rest.
    let txns_to_resplit: Vec<Transaction> = std::mem::take(&mut processed.txns_to_resplit);

    // Create actionable updates for the processed results.
    let (add_updates, split_updates) = create_updates(processed, config.creditor.proxy_category_id);

    // Prepare final output data.
    let mut batched_txn_info: Vec<(TransactionId, Txn)> = vec![];

    // Execute adds and append results to output.
    {
        let (mut added_ids_and_email_txns, mut added_issues) =
            execute_adds(add_updates, api).await;
        batched_txn_info.append(&mut added_ids_and_email_txns);
        issues.append(&mut added_issues);
    }

    // Execute splits and append results to output.
    {
        let (mut added_ids_and_email_txns, mut added_issues) =
            execute_splits(split_updates, api).await;
        batched_txn_info.append(&mut added_ids_and_email_txns);
        issues.append(&mut added_issues);
    }

    // Execute resplits: re-split parent transactions to split tagged children.
    {
        let (mut added_ids_and_email_txns, mut added_issues) =
            execute_resplits(txns_to_resplit, &all_txns, config.creditor.proxy_category_id, api)
                .await;
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
                (ids, txns, tot)
            },
        );

    // Create batch id and save to local data.
    let batch_id = Uuid::new_v4().to_string();
    let batch: Batch = Batch {
        id: Uuid::new_v4().to_string(),
        amount: total_amount,
        transaction_ids: batched_ids.clone(),
        reconciliation: None,
    };
    persistence.save_batch(&batch)?;

    // Send the batch notification.
    let email_warnings: Vec<String> = issues.iter().map(|i| i.to_string()).collect();
    notifier
        .send_batch_notification(&batch_id, &total_amount, &email_txns, email_warnings.clone())
        .await?;

    // Dispatch to plugins.
    plugins
        .dispatch(&crate::plugin::batch_created_message(
            &batch_id,
            &total_amount,
            &email_txns,
            &email_warnings,
        ))
        .await;

    tracing::info!(
        batch_id,
        amount = %total_amount,
        transaction_count = batched_ids.len(),
        warnings = issues.len(),
        "Batch created"
    );
    Ok(())
}

// Execute adding these transactions to the batch with their associated pre-prepared update.
// Return info about the added transactions and any issues encountered during the operation.
async fn execute_adds(
    txns_and_updates: Vec<(crate::lunch_money::model::transaction::Transaction, TransactionUpdate)>,
    api: &(impl LunchMoney + Sync),
) -> (Vec<(TransactionId, Txn)>, Vec<Issue>) {
    let mut batched_txn_info: Vec<(TransactionId, Txn)> = vec![];
    let mut issues: Vec<Issue> = vec![];

    for (txn, update) in txns_and_updates {
        let result = api.update_transaction(update).await;
        match result {
            Ok(_) => {
                batched_txn_info.push((
                    txn.id,
                    Txn {
                        payee: txn.payee,
                        amount: txn.amount,
                        date: txn.date,
                        notes: txn.notes,
                    },
                ));
            }
            Err(e) => {
                tracing::warn!(txn_id = txn.id, error = %e, "Failed to update transaction");
                issues.push(Issue::TransactionUpdateError(txn.id, e.to_string()));
            }
        }
    }

    (batched_txn_info, issues)
}

// Execute splitting and adding these transactions to the batch with their associated pre-prepared update.
// Returns info about the transactions added to the batch - i.e. after splitting,
// return the debtor's split txn info
async fn execute_splits(
    txns_and_updates: Vec<(
        crate::lunch_money::model::transaction::Transaction,
        TransactionAndSplitUpdate,
    )>,
    api: &(impl LunchMoney + Sync),
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

        let result = api.update_transaction_and_split(update).await;

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
                                notes: txn.notes
                            },
                        ));
                    }
                    Err(e) => {
                        tracing::warn!(txn_id = txn.id, error = %e, "Split response missing expected ID");
                        issues.push(Issue::TransactionUpdateError(txn.id, e.to_string()));
                    }
                }
            }
            Err(e) => {
                tracing::warn!(txn_id = txn.id, error = %e, "Failed to split transaction");
                issues.push(Issue::TransactionUpdateError(txn.id, e.to_string()));
            }
        }
    }

    (batched_txn_info, issues)
}

/// Re-split parent transactions to split tagged child transactions.
///
/// For each tagged child, we replace it in its parent's split list with two new
/// children (creditor half + debtor half), preserving all other siblings.
async fn execute_resplits(
    txns_to_resplit: Vec<Transaction>,
    all_txns: &[Transaction],
    proxy_category_id: u32,
    api: &(impl LunchMoney + Sync),
) -> (Vec<(TransactionId, Txn)>, Vec<Issue>) {
    let mut batched_txn_info: Vec<(TransactionId, Txn)> = vec![];
    let mut issues: Vec<Issue> = vec![];

    // Group tagged children by parent_id.
    let mut by_parent: HashMap<TransactionId, Vec<Transaction>> = HashMap::new();
    for child in txns_to_resplit {
        let parent_id = child.parent_id.expect("resplit txn must have parent_id");
        by_parent.entry(parent_id).or_default().push(child);
    }

    for (parent_id, tagged_children) in by_parent {
        let tagged_ids: Vec<TransactionId> =
            tagged_children.iter().map(|t| t.id).collect();

        // Find siblings (other children of the same parent) in the fetched transactions.
        let siblings: Vec<Transaction> = all_txns
            .iter()
            .filter(|t| t.parent_id == Some(parent_id) && !tagged_ids.contains(&t.id))
            .cloned()
            .collect();

        tracing::info!(
            parent_id,
            tagged_count = tagged_children.len(),
            sibling_count = siblings.len(),
            "Re-splitting parent transaction"
        );

        let (split_items, debtor_amounts) =
            create_resplit_items(&tagged_children, &siblings, proxy_category_id);

        let result = api.update_split((parent_id, split_items)).await;

        match result {
            Ok(split_response) => {
                for (i, child) in tagged_children.iter().enumerate() {
                    // Debtor halves are at odd indices: 1, 3, 5, ...
                    let debtor_index = 2 * i + 1;
                    match split_response.split_ids.get(debtor_index) {
                        Some(&debtor_id) => {
                            batched_txn_info.push((
                                debtor_id,
                                Txn {
                                    payee: child.payee.clone(),
                                    amount: debtor_amounts[i],
                                    date: child.date,
                                    notes: child.notes.clone(),
                                },
                            ));
                        }
                        None => {
                            let msg = format!(
                                "resplit response missing debtor ID at index {} for child {}",
                                debtor_index, child.id
                            );
                            tracing::warn!(txn_id = child.id, msg, "Resplit response missing expected ID");
                            issues.push(Issue::TransactionUpdateError(child.id, msg));
                        }
                    }
                }
            }
            Err(e) => {
                let msg = e.to_string();
                tracing::warn!(parent_id, error = %msg, "Failed to resplit parent transaction");
                for child in &tagged_children {
                    issues.push(Issue::TransactionUpdateError(child.id, msg.clone()));
                }
            }
        }
    }

    (batched_txn_info, issues)
}
