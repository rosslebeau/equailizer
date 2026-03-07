use crate::{
    config::Config,
    date_helpers,
    lunch_money::{
        api::{update_transaction::SplitUpdateItem, LunchMoney},
        model::transaction::Transaction,
    },
    persist::{Batch, Persistence, Settlement},
    usd::USD,
};
use anyhow::Result;

pub async fn reconcile_all(
    config: &Config,
    creditor_api: &(impl LunchMoney + Sync),
    debtor_api: &(impl LunchMoney + Sync),
    persistence: &(impl Persistence + Sync),
) -> Result<()> {
    let unreconciled = persistence.unreconciled_batches()?;
    for batch in unreconciled {
        reconcile_batch(batch, config, creditor_api, debtor_api, persistence).await?;
    }
    Ok(())
}

pub async fn reconcile_batch_name(
    batch_name: &str,
    config: &Config,
    creditor_api: &(impl LunchMoney + Sync),
    debtor_api: &(impl LunchMoney + Sync),
    persistence: &(impl Persistence + Sync),
) -> Result<()> {
    reconcile_batch(
        persistence.get_batch(batch_name)?,
        config,
        creditor_api,
        debtor_api,
        persistence,
    )
    .await
}

async fn reconcile_batch(
    batch: Batch,
    config: &Config,
    creditor_api: &(impl LunchMoney + Sync),
    debtor_api: &(impl LunchMoney + Sync),
    persistence: &(impl Persistence + Sync),
) -> Result<()> {
    let span = tracing::info_span!("Reconcile Batch");
    let _enter = span.enter();
    tracing::debug!(batch.id, "Starting");

    let batch_txns = creditor_api
        .get_transactions_by_id(&batch.transaction_ids)
        .await?;

    // Find the last transaction date to limit our settlement search window.
    let last_txn_date = batch_txns
        .iter()
        .map(|txn| txn.date)
        .max()
        .ok_or_else(|| anyhow::anyhow!("no creditor transactions while trying to reconcile"))?;

    // Find the settlement credit on the creditor's side.
    let creditor_txns = creditor_api
        .get_transactions(last_txn_date, date_helpers::now_date_naive_eastern())
        .await?;
    let settlement_credit = find_settlement_transaction(
        &creditor_txns,
        -batch.amount,
        config.creditor.settlement_account_id,
    )
    .ok_or_else(|| anyhow::anyhow!("did not find suitable settlement credit"))?
    .clone();

    // Find the settlement debit on the debtor's side.
    let debtor_txns = debtor_api
        .get_transactions(last_txn_date, date_helpers::now_date_naive_eastern())
        .await?;
    let settlement_debit = find_settlement_transaction(
        &debtor_txns,
        batch.amount,
        config.debtor.settlement_account_id,
    )
    .ok_or_else(|| anyhow::anyhow!("did not find suitable settlement debit"))?
    .clone();

    // Split out the creditor's side to match the transactions in the batch.
    let creditor_splits = build_creditor_splits(
        &batch_txns,
        &config.debtor.name,
        config.creditor.proxy_category_id,
    );
    creditor_api
        .update_split((settlement_credit.id, creditor_splits))
        .await?;

    // Split out the debtor's side to match the transactions in the batch.
    let debtor_splits = build_debtor_splits(&batch_txns);
    debtor_api
        .update_split((settlement_debit.id, debtor_splits))
        .await?;

    // Save batch so we know it's reconciled.
    persistence.save_batch(&Batch {
        id: batch.id,
        amount: batch.amount,
        transaction_ids: batch.transaction_ids,
        reconciliation: Some(Settlement {
            settlement_credit_id: settlement_credit.id,
            settlement_debit_id: settlement_debit.id,
        }),
    })?;

    tracing::debug!("Finished");
    Ok(())
}

/// Find a transaction matching the expected amount in the given settlement account.
pub fn find_settlement_transaction(
    candidates: &[Transaction],
    expected_amount: USD,
    settlement_account_id: u32,
) -> Option<&Transaction> {
    candidates.iter().find(|t| {
        t.amount == expected_amount
            && t.plaid_account_id
                .is_some_and(|acct| acct == settlement_account_id)
    })
}

/// Build creditor settlement splits: negative amounts, debtor name as payee, proxy category.
pub fn build_creditor_splits(
    batch_txns: &[Transaction],
    debtor_name: &str,
    proxy_category_id: u32,
) -> Vec<SplitUpdateItem> {
    batch_txns
        .iter()
        .map(|t| SplitUpdateItem {
            amount: -t.amount,
            payee: Some(debtor_name.to_string()),
            category_id: Some(proxy_category_id),
            notes: Some(t.payee.clone()),
            date: Some(t.date),
        })
        .collect()
}

/// Build debtor settlement splits: original amounts, payees, and notes passed through.
pub fn build_debtor_splits(batch_txns: &[Transaction]) -> Vec<SplitUpdateItem> {
    batch_txns
        .iter()
        .map(|t| SplitUpdateItem {
            amount: t.amount,
            payee: Some(t.payee.to_owned()),
            category_id: None,
            notes: t.notes.clone(),
            date: Some(t.date),
        })
        .collect()
}
