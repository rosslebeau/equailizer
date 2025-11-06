use crate::{
    config::{self, Config},
    date_helpers,
    lunch_money::{
        api::update_transaction::{SplitUpdateItem, TransactionUpdateItem},
        model::transaction::{Transaction, TransactionStatus},
    },
    persist::{self, Batch, Settlement},
};

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

async fn reconcile_batch(
    batch: Batch,
    config: &Config,
    profile: &String,
) -> Result<(), Box<dyn std::error::Error>> {
    let span = tracing::info_span!("Reconcile Batch");
    let _enter = span.enter();
    tracing::debug!(batch.id, "Starting");

    let creditor_client = crate::lunch_money::api::Client {
        auth_token: config.creditor.api_key.to_owned(),
    };
    let debtor_client = crate::lunch_money::api::Client {
        auth_token: config.debtor.api_key.to_owned(),
    };

    let batch_txns = creditor_client
        .get_transactions_by_id(&batch.transaction_ids)
        .await?;

    // Now that we have the batched transactions, we need to find the
    // settlement transactions (settlement credit/debit).
    //
    // In order to find these, we should look through all of the
    // transactions that have happened since the last item in the batch.
    let last_txn_date = batch_txns
        .iter()
        .map(|txn| txn.date)
        .reduce(|acc, date| if date > acc { date } else { acc })
        .ok_or("no creditor transactions while trying to reconcile")?;

    // Scan new transactions from the creditor to look for one that
    // matches the amount of the batch, and is in the creditor's designated
    // account for settlements (from config).
    let settlement_credit = creditor_client
        .get_transactions(last_txn_date, date_helpers::now_date_naive_eastern())
        .await?
        .iter()
        .filter(|t| {
            t.amount == -batch.amount
                && t.plaid_account_id
                    .is_some_and(|acct| acct == config.creditor.settlement_account_id)
        })
        .collect::<Vec<&Transaction>>()
        .first()
        .ok_or("did not find suitable settlement credit")?
        .to_owned()
        .to_owned();

    // Scan new transactions from the debtor to look for one that
    // matches the amount of the batch, and is in the debtor's designated
    // account for settlements (from config).
    let settlement_debit = debtor_client
        .get_transactions(last_txn_date, date_helpers::now_date_naive_eastern())
        .await?
        .iter()
        .filter(|t| {
            t.amount == batch.amount
                && t.plaid_account_id
                    .is_some_and(|acct| acct == config.debtor.settlement_account_id)
        })
        .collect::<Vec<&Transaction>>()
        .first()
        .ok_or("did not find suitable settlement debit")?
        .to_owned()
        .to_owned();

    // Split out the creditor's side to match the transactions in the batch
    // so there are side-by-side debits and credits for each item.
    let creditor_splits: Vec<SplitUpdateItem> = batch_txns
        .iter()
        .map({
            |t| SplitUpdateItem {
                amount: t.amount,
                payee: Some(config.debtor.name.clone()),
                category_id: Some(config.creditor.proxy_category_id),
                notes: Some("equailizer".to_string()),
                date: Some(t.date),
            }
        })
        .collect();

    creditor_client
        .update_split((settlement_credit.id, creditor_splits))
        .await?;

    // Split out the debtor's side to match the transactions in the batch.
    // Use this opportunity to pass through the payee and notes for the
    // debtor to use when they categorize these.
    let debtor_splits: Vec<SplitUpdateItem> = batch_txns
        .iter()
        .map({
            |t| SplitUpdateItem {
                amount: t.amount,
                payee: Some(t.payee.to_owned()),
                category_id: None,
                notes: t.notes.clone(),
                date: Some(t.date),
            }
        })
        .collect();

    debtor_client
        .update_split((settlement_debit.id, debtor_splits))
        .await?;

    // Save batch so we know it's reconciled and can refer to it
    // later if we want, from the data persistence.
    persist::save_batch(
        &Batch {
            id: batch.id,
            amount: batch.amount,
            transaction_ids: batch.transaction_ids,
            reconciliation: Some(Settlement {
                settlement_credit_id: settlement_credit.id,
                settlement_debit_id: settlement_debit.id,
            }),
        },
        profile,
    )?;

    tracing::debug!("Finished");
    Ok(())
}
