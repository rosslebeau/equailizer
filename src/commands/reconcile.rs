use crate::{
    config::{self, Config},
    date_helpers,
    issue::Issue,
    lunch_money::{
        api::{
            update_transaction::{SplitUpdateItem, TransactionUpdateItem},
            LunchMoney,
        },
        model::transaction::{Transaction, TransactionId, TransactionStatus},
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
) -> Result<Vec<Issue>> {
    let unreconciled = persistence.unreconciled_batches()?;
    let total = unreconciled.len();
    tracing::info!(unreconciled_batches = total, "Starting reconcile-all");

    if total == 0 {
        tracing::info!("No unreconciled batches found");
        return Ok(vec![]);
    }

    let mut reconciled = 0u32;
    let mut issues: Vec<Issue> = vec![];
    for batch in unreconciled {
        let batch_id = batch.id.clone();
        match reconcile_batch(batch, config, creditor_api, debtor_api, persistence).await {
            Ok(()) => reconciled += 1,
            Err(e) => {
                tracing::warn!(batch_id, error = %e, "Failed to reconcile batch");
                issues.push(Issue::BatchReconcileError(batch_id, format!("{e:#}")));
            }
        }
    }

    tracing::info!(reconciled, failed = issues.len(), total, "Reconcile-all complete");
    Ok(issues)
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
    if batch.reconciliation.is_some() {
        anyhow::bail!("batch '{}' is already reconciled", batch.id);
    }

    let span = tracing::info_span!("Reconcile Batch", batch_id = %batch.id);
    let _enter = span.enter();

    tracing::info!(
        batch_id = %batch.id,
        amount = %batch.amount,
        transaction_count = batch.transaction_ids.len(),
        "Starting batch reconciliation"
    );

    let batch_txns = creditor_api
        .get_transactions_by_id(&batch.transaction_ids)
        .await?;

    // Find the last transaction date to limit our settlement search window.
    let last_txn_date = batch_txns
        .iter()
        .map(|txn| txn.date)
        .max()
        .ok_or_else(|| anyhow::anyhow!("no creditor transactions while trying to reconcile"))?;

    let search_end = date_helpers::now_date_naive_eastern();
    tracing::debug!(
        search_start = %last_txn_date.format("%Y-%m-%d"),
        search_end = %search_end.format("%Y-%m-%d"),
        "Searching for settlement transactions"
    );

    // Find the settlement credit on the creditor's side.
    let creditor_txns = creditor_api
        .get_transactions(last_txn_date, search_end)
        .await?;
    let settlement_credit = find_settlement_transaction(
        &creditor_txns,
        -batch.amount,
        config.creditor.settlement_account_id,
    )
    .ok_or_else(|| anyhow::anyhow!(
        "did not find settlement credit for {} in {} creditor transactions (account {})",
        -batch.amount, creditor_txns.len(), config.creditor.settlement_account_id
    ))?
    .clone();

    tracing::info!(
        settlement_credit_id = settlement_credit.id,
        "Found creditor settlement"
    );

    // Find the settlement debit on the debtor's side.
    let debtor_txns = debtor_api
        .get_transactions(last_txn_date, search_end)
        .await?;
    let settlement_debit = find_settlement_transaction(
        &debtor_txns,
        batch.amount,
        config.debtor.settlement_account_id,
    )
    .ok_or_else(|| anyhow::anyhow!(
        "did not find settlement debit for {} in {} debtor transactions (account {})",
        batch.amount, debtor_txns.len(), config.debtor.settlement_account_id
    ))?
    .clone();

    tracing::info!(
        settlement_debit_id = settlement_debit.id,
        "Found debtor settlement"
    );

    // Split out the creditor's side to match the transactions in the batch.
    let creditor_splits = build_creditor_splits(
        &batch_txns,
        &config.debtor.name,
        config.creditor.proxy_category_id,
    );
    let creditor_split_response = creditor_api
        .update_split((settlement_credit.id, creditor_splits))
        .await?;

    // Split out the debtor's side to match the transactions in the batch.
    let debtor_splits = build_debtor_splits(&batch_txns);
    let debtor_split_response = debtor_api
        .update_split((settlement_debit.id, debtor_splits))
        .await?;

    // Clear settlement parents and all split children.
    clear_transactions(&[settlement_credit.id], creditor_api).await?;
    clear_transactions(&creditor_split_response.split_ids, creditor_api).await?;
    clear_transactions(&[settlement_debit.id], debtor_api).await?;
    clear_transactions(&debtor_split_response.split_ids, debtor_api).await?;

    // Remove the pending reconciliation tag from batch transactions.
    remove_pending_tags(&batch_txns, creditor_api).await?;

    // Save batch so we know it's reconciled.
    persistence.save_batch(&Batch {
        id: batch.id.clone(),
        amount: batch.amount,
        transaction_ids: batch.transaction_ids,
        reconciliation: Some(Settlement {
            settlement_credit_id: settlement_credit.id,
            settlement_debit_id: settlement_debit.id,
        }),
    })?;

    tracing::info!(
        batch_id = %batch.id,
        settlement_credit_id = settlement_credit.id,
        settlement_debit_id = settlement_debit.id,
        "Batch reconciled"
    );
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

/// Mark each transaction as cleared.
async fn clear_transactions(
    ids: &[TransactionId],
    api: &(impl LunchMoney + Sync),
) -> Result<()> {
    for &id in ids {
        api.update_transaction((
            id,
            TransactionUpdateItem {
                payee: None,
                category_id: None,
                notes: None,
                tags: None,
                status: Some(TransactionStatus::Cleared),
            },
        ))
        .await?;
    }
    tracing::debug!(count = ids.len(), ?ids, "Cleared transactions");
    Ok(())
}

/// Remove the pending reconciliation tag from each batch transaction.
async fn remove_pending_tags(
    batch_txns: &[Transaction],
    api: &(impl LunchMoney + Sync),
) -> Result<()> {
    let pending_tag = config::TAG_PENDING_RECONCILIATION;
    let mut removed = 0u32;

    for txn in batch_txns {
        if !txn.tag_names().contains(&&pending_tag.to_string()) {
            continue;
        }

        let tags: Vec<String> = txn
            .tags
            .iter()
            .map(|t| t.name.clone())
            .filter(|name| name != pending_tag)
            .collect();

        api.update_transaction((
            txn.id,
            TransactionUpdateItem {
                payee: None,
                category_id: None,
                notes: None,
                tags: Some(tags),
                status: None,
            },
        ))
        .await?;

        removed += 1;
    }

    tracing::info!(removed, total = batch_txns.len(), "Removed pending reconciliation tags");
    Ok(())
}
