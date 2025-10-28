use crate::config::Config;
use crate::lunch_money::api::update_transaction::{Split, TransactionUpdate};
use crate::lunch_money::model::transaction::*;
use crate::persist::{Batch, Reconciliation};
use crate::{date_helpers, persist};

// On success, returns a list of reconciled batch names
pub async fn reconcile_all(
    config: &Config,
    profile: &String,
) -> Result<(), Box<dyn std::error::Error>> {
    let unreconciled = persist::unreconciled_batches(profile)?;
    for batch in unreconciled {
        reconcile_batch(batch, config, profile).await?;
    }
    Ok(())
}

pub async fn reconcile_batch_name(
    batch_name: &String,
    config: &Config,
    profile: &String,
) -> Result<(), Box<dyn std::error::Error>> {
    reconcile_batch(persist::get_batch(batch_name, profile)?, config, profile).await
}

pub async fn reconcile_batch(
    batch: Batch,
    config: &Config,
    profile: &String,
) -> Result<(), Box<dyn std::error::Error>> {
    let span = tracing::info_span!("Reconcile Batch");
    let _enter = span.enter();
    tracing::debug!(batch.name, "Starting reconcile batch");

    let lm_creditor_client = crate::lunch_money::api::Client {
        auth_token: config.creditor.api_key.to_owned(),
    };

    let creditor_batch_txns = lm_creditor_client
        .get_transactions_by_id(&batch.transaction_ids)
        .await?;

    // Find creditor's reconciliation txn
    // Transaction must have occurred between the last txn in the batch and the current date
    let creditor_new_txns = lm_creditor_client
        .get_transactions(batch.end_date, date_helpers::now_date_naive_eastern())
        .await?;

    let creditor_reconciliation_txn = creditor_new_txns
        .iter()
        .filter(|t| {
            t.amount == -batch.amount
                && t.plaid_account_id
                    .is_some_and(|acct| acct == config.creditor.repayment_account_id)
        })
        .collect::<Vec<&Transaction>>()
        .first()
        .ok_or("did not find suitable creditor reconciliation transaction")?
        .to_owned()
        .to_owned();

    let repayment_txn_update = TransactionUpdate {
        payee: None,
        category_id: Some(config.creditor.proxy_category_id),
        notes: Some(batch.name.to_owned()),
        tags: None,
        status: Some(TransactionStatus::Cleared),
    };
    lm_creditor_client
        .update_txn_only(creditor_reconciliation_txn.id, &repayment_txn_update)
        .await?;

    let lm_debtor_client = crate::lunch_money::api::Client {
        auth_token: config.debtor.api_key.to_owned(),
    };

    // Get txns for the debtor that have happened between the batch creation and now
    // The repayment txn can't have happened before the last txn in the batch
    let debtor_txns = lm_debtor_client
        .get_transactions(batch.end_date, date_helpers::now_date_naive_eastern())
        .await?;

    let debtor_repayment_txn = debtor_txns
        .iter()
        .filter(|t| {
            t.amount == batch.amount
                && t.plaid_account_id
                    .is_some_and(|acct| acct == config.debtor.repayment_account_id)
        })
        .collect::<Vec<&Transaction>>()
        .first()
        .ok_or("no suitable debtor reconciliation transaction found")?
        .to_owned()
        .to_owned();

    let debtor_splits: Vec<Split> = create_debtor_splits(&creditor_batch_txns);

    let debtor_txn_update = TransactionUpdate {
        payee: None,
        category_id: None,
        notes: Some(batch.name.to_owned()),
        tags: None,
        status: None,
    };

    lm_debtor_client
        .update_txn_and_split(debtor_repayment_txn.id, &debtor_txn_update, &debtor_splits)
        .await?;

    let updated_batch = Batch {
        name: batch.name,
        start_date: batch.start_date,
        end_date: batch.end_date,
        amount: batch.amount,
        transaction_ids: batch.transaction_ids,
        reconciliation: Some(Reconciliation {
            creditor_repayment_transaction_id: creditor_reconciliation_txn.id,
            debtor_repayment_transaction_id: debtor_repayment_txn.id,
        }),
    };

    persist::save_batch(&updated_batch, profile)?;

    tracing::info!(updated_batch.name, "Finished reconcile batch");

    return Ok(());
}

// pass through the payees and notes so they can have info to categorize
fn create_debtor_splits(creditor_proxy_txns: &Vec<Transaction>) -> Vec<Split> {
    creditor_proxy_txns
        .iter()
        .map({
            |t| Split {
                amount: t.amount,
                payee: Some(t.payee.to_owned()),
                category_id: None,
                notes: Some(
                    t.notes
                        .to_owned()
                        .map_or("Paid via equailizer".to_string(), |notes| {
                            format!("Paid via equailizer. Notes: {:?}", notes)
                        }),
                ),
                date: Some(t.date),
            }
        })
        .collect()
}
