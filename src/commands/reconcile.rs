use crate::{
    config::Config,
    date_helpers,
    error::{Error, Result},
    lunch_money::{
        api::{
            update_transaction::{SplitUpdateItem, TransactionUpdateItem},
            LunchMoney,
        },
        model::transaction::{Transaction, TransactionId, TransactionStatus},
    },
    persist::{Batch, Persistence, Settlement},
    plugin::PluginManager,
    usd::USD,
};

pub struct ReconcileAllResult {
    pub reconciled: u32,
    pub errors: Vec<Error>,
}

pub async fn reconcile_all(
    config: &Config,
    creditor_api: &(impl LunchMoney + Sync),
    debtor_api: &(impl LunchMoney + Sync),
    persistence: &(impl Persistence + Sync),
    plugins: &mut PluginManager,
) -> Result<ReconcileAllResult> {
    let unreconciled = persistence.unreconciled_batches()?;
    let total = unreconciled.len();
    tracing::info!(unreconciled_batches = total, "Starting reconcile-all");

    if total == 0 {
        tracing::info!("No unreconciled batches found");
        return Ok(ReconcileAllResult {
            reconciled: 0,
            errors: vec![],
        });
    }

    let mut reconciled = 0u32;
    let mut errors: Vec<Error> = vec![];
    for batch in unreconciled {
        let batch_id = batch.id.clone();
        match reconcile_batch(batch, config, creditor_api, debtor_api, persistence, plugins).await {
            Ok(()) => reconciled += 1,
            Err(e) => {
                tracing::warn!(batch_id, error = %e, "Failed to reconcile batch");
                errors.push(Error::BatchReconcile {
                    batch_id,
                    source: Box::new(e),
                });
            }
        }
    }

    tracing::info!(reconciled, failed = errors.len(), total, "Reconcile-all complete");
    Ok(ReconcileAllResult { reconciled, errors })
}

pub async fn reconcile_batch_name(
    batch_name: &str,
    config: &Config,
    creditor_api: &(impl LunchMoney + Sync),
    debtor_api: &(impl LunchMoney + Sync),
    persistence: &(impl Persistence + Sync),
    plugins: &mut PluginManager,
) -> Result<()> {
    reconcile_batch(
        persistence.get_batch(batch_name)?,
        config,
        creditor_api,
        debtor_api,
        persistence,
        plugins,
    )
    .await
}

async fn reconcile_batch(
    batch: Batch,
    config: &Config,
    creditor_api: &(impl LunchMoney + Sync),
    debtor_api: &(impl LunchMoney + Sync),
    persistence: &(impl Persistence + Sync),
    plugins: &mut PluginManager,
) -> Result<()> {
    if batch.reconciliation.is_some() {
        return Err(Error::BatchAlreadyReconciled(batch.id));
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
        .ok_or(Error::NoTransactionsFound)?;

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
    .ok_or_else(|| Error::SettlementNotFound {
        side: "credit",
        batch_id: batch.id.clone(),
    })?
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
    .ok_or_else(|| Error::SettlementNotFound {
        side: "debit",
        batch_id: batch.id.clone(),
    })?
    .clone();

    tracing::info!(
        settlement_debit_id = settlement_debit.id,
        "Found debtor settlement"
    );

    // Split the creditor settlement, or recover existing children if a previous
    // reconcile attempt was interrupted after splitting.
    let creditor_split_ids_to_clear: Vec<TransactionId> = if settlement_credit.has_children {
        tracing::info!(
            parent_id = settlement_credit.id,
            "Creditor settlement already split; reusing existing children"
        );
        let min_batch_date = batch_txns
            .iter()
            .map(|t| t.date)
            .min()
            .ok_or(Error::NoTransactionsFound)?;
        let recovery_txns = creditor_api
            .get_transactions(min_batch_date, search_end)
            .await?;
        let existing = find_existing_split_children(
            &recovery_txns,
            settlement_credit.id,
            batch_txns.len(),
        )?;
        existing
            .into_iter()
            .filter(|t| t.status != TransactionStatus::Cleared)
            .map(|t| t.id)
            .collect()
    } else {
        let creditor_splits = build_creditor_splits(
            &batch_txns,
            &config.debtor.name,
            config.creditor.proxy_category_id,
        );
        creditor_api
            .update_split((settlement_credit.id, creditor_splits))
            .await?
            .split_ids
    };

    // Split the debtor settlement, or skip if a previous attempt already split it.
    // We don't need debtor child IDs because we intentionally don't clear them.
    if !settlement_debit.has_children {
        let debtor_splits = build_debtor_splits(&batch_txns);
        debtor_api
            .update_split((settlement_debit.id, debtor_splits))
            .await?;
    } else {
        tracing::info!(
            parent_id = settlement_debit.id,
            "Debtor settlement already split; skipping split call"
        );
    }

    // Clear creditor settlement parent and uncleared split children.
    if settlement_credit.status != TransactionStatus::Cleared {
        clear_transactions(&[settlement_credit.id], creditor_api).await?;
    }
    clear_transactions(&creditor_split_ids_to_clear, creditor_api).await?;
    // Clear only the debtor settlement parent — leave split children uncleared
    // so the debtor can categorize them.
    if settlement_debit.status != TransactionStatus::Cleared {
        clear_transactions(&[settlement_debit.id], debtor_api).await?;
    }

    // Dispatch to plugins before saving (which moves batch fields).
    plugins
        .dispatch(&crate::plugin::batch_reconciled_message(
            &batch,
            settlement_credit.id,
            settlement_debit.id,
        ))
        .await;

    let batch_id = batch.id.clone();

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

    tracing::info!(
        batch_id,
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

/// Find existing split children of `parent_id` in `candidates`, used during
/// recovery from an interrupted reconcile. Errors on count mismatch — a wrong
/// count signals either a stale fetch window or a corrupted split state, and
/// must be investigated manually rather than silently proceeding.
pub fn find_existing_split_children(
    candidates: &[Transaction],
    parent_id: TransactionId,
    expected_count: usize,
) -> Result<Vec<Transaction>> {
    let children: Vec<Transaction> = candidates
        .iter()
        .filter(|t| t.parent_id == Some(parent_id))
        .cloned()
        .collect();
    if children.len() != expected_count {
        return Err(Error::Api(format!(
            "expected {expected_count} split children for transaction {parent_id}, found {}",
            children.len()
        )));
    }
    Ok(children)
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
