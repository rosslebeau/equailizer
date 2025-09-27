use crate::config::Config;
use crate::lunch_money::api::update_transaction::{Split, TransactionUpdate};
use crate::lunch_money::model::transaction::*;
use crate::usd::USD;
use crate::{date_helpers, persist};
use chrono::NaiveDate;
use rust_decimal::prelude::*;

struct CreditorBatch {
    repayment_txn: Transaction,
    proxy_txns: Vec<Transaction>,
}

// On success, returns a list of reconciled batch names
pub async fn reconcile_all(
    config: &Config,
    profile: &String,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let unreconciled = persist::unreconciled_metas(profile)?;
    for meta in &unreconciled {
        reconcile_batch(
            &meta.name,
            meta.start_date,
            date_helpers::now_date_naive_eastern(),
            config,
            profile,
        )
        .await?;
    }
    Ok(unreconciled.iter().map(|m| m.name.to_owned()).collect())
}

pub async fn reconcile_batch(
    batch_name: &String,
    search_start_date: NaiveDate,
    search_end_date: NaiveDate,
    config: &Config,
    profile: &String,
) -> Result<(), Box<dyn std::error::Error>> {
    let lm_creditor_client = crate::lunch_money::api::Client {
        auth_token: config.creditor.api_key.to_owned(),
    };

    let creditor_txns = lm_creditor_client
        .get_transactions(search_start_date, search_end_date)
        .await?;

    let creditor_batch = get_creditor_batch_from_txns(creditor_txns, &batch_name, config).await?;

    let batch_total = creditor_batch
        .proxy_txns
        .iter()
        .map(|t| t.amount)
        .fold(USD::new(dec!(0)), |acc, amt| acc + amt);

    if (batch_total + creditor_batch.repayment_txn.amount) != USD::new(dec!(0)) {
        return Err("batch total is not equal to repayment transaction".into());
    }

    let repayment_txn_update = TransactionUpdate {
        payee: None,
        category_id: Some(config.creditor.proxy_category_id),
        notes: Some(batch_name.to_owned()),
        tags: None,
        status: Some(TransactionStatus::Cleared),
    };
    lm_creditor_client
        .update_txn_only(creditor_batch.repayment_txn.id, &repayment_txn_update)
        .await?;

    let lm_debtor_client = crate::lunch_money::api::Client {
        auth_token: config.debtor.api_key.to_owned(),
    };
    let debtor_txns = lm_debtor_client
        .get_transactions(search_start_date, search_end_date)
        .await?;

    let debtor_repayment_txn =
        get_debtor_repayment_txn_from_txns(debtor_txns, &batch_name, config)?;

    // create splits on debtor's side to pass through the payees so they can categorize
    let debtor_splits: Vec<Split> = create_debtor_splits(&creditor_batch.proxy_txns);

    let debtor_txn_update = TransactionUpdate {
        payee: None,
        category_id: None,
        notes: Some(batch_name.to_owned()),
        tags: None,
        status: None,
    };

    lm_debtor_client
        .update_txn_and_split(debtor_repayment_txn.id, &debtor_txn_update, &debtor_splits)
        .await?;

    persist::set_reconciled(&batch_name, true, profile)?;

    return Ok(());
}

fn is_in_acct(txn: &Transaction, account_id: u32) -> bool {
    match txn.plaid_account_id {
        Some(acct_id) => acct_id == account_id,
        None => false,
    }
}

async fn get_creditor_batch_from_txns(
    mut txns: Vec<Transaction>,
    batch_name: &String,
    config: &Config,
) -> Result<CreditorBatch, Box<dyn std::error::Error>> {
    // Look for transactions that have the batch name in either:
    // - the payee (repayment txn), or
    // - the notes (previously batched proxy txns)
    txns.retain(|t| {
        t.payee.contains(batch_name) || t.notes.as_ref().is_some_and(|n| n.contains(batch_name))
    });

    // Find the first transaction on the repayment account that has this batch name and a negative amount remove it from the vec
    // There should only be one, we'll check the balance of the batch later
    let repayment_txn = match txns.iter().position(|t| {
        is_in_acct(t, config.creditor.repayment_account_id) && t.amount.value() < dec!(0)
    }) {
        Some(position) => txns.swap_remove(position),
        None => return Err("didn't find creditor repayment transaction".into()),
    };

    return Ok(CreditorBatch {
        repayment_txn: repayment_txn,
        proxy_txns: txns,
    });
}

fn get_debtor_repayment_txn_from_txns(
    mut txns: Vec<Transaction>,
    batch_name: &String,
    config: &Config,
) -> Result<Transaction, Box<dyn std::error::Error>> {
    if let Some(position) = txns
        .iter()
        .position(|t| matches_debtor_txn(t, batch_name, config))
    {
        return Ok(txns.swap_remove(position));
    } else {
        return Err("didn't find debtor repayment transaction".into());
    }
}

fn matches_debtor_txn(txn: &Transaction, batch_name: &String, config: &Config) -> bool {
    is_in_acct(txn, config.debtor.repayment_account_id)
        && (txn.payee.contains(batch_name)
            || txn
                .original_name
                .as_ref()
                .is_some_and(|x| x.contains(batch_name)))
}

fn create_debtor_splits(creditor_proxy_txns: &Vec<Transaction>) -> Vec<Split> {
    creditor_proxy_txns
        .iter()
        .map({
            |t| Split {
                amount: t.amount,
                payee: Some(t.payee.to_owned()),
                category_id: None,
                notes: Some("Paid via equailizer".to_string()),
                date: Some(t.date),
            }
        })
        .collect()
}
