use crate::config::{Config, eq_batch_name};
use crate::lunch_money::api::update_transaction::{Split, TransactionUpdate};
use crate::lunch_money::model::transaction::*;
use crate::usd::USD;
use chrono::NaiveDate;
use rust_decimal::prelude::*;
use uuid::{self, Uuid};

struct CreditorBatch {
    repayment_txn: Transaction,
    proxy_txns: Vec<Transaction>,
}

pub async fn run(
    batch_name: &String,
    start_date: NaiveDate,
    end_date: NaiveDate,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let lm_creditor_client = crate::lunch_money::api::Client {
        auth_token: config.creditor.api_key.to_owned(),
    };

    let creditor_txns = lm_creditor_client
        .get_transactions(start_date, end_date)
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
        notes: None,
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
        .get_transactions(start_date, end_date)
        .await?;

    let debtor_repayment_txn =
        get_debtor_repayment_txn_from_txns(debtor_txns, &batch_name, config)?;

    // create splits on debtor's side to pass through the payees so they can categorize
    let debtor_splits: Vec<Split> = get_debtor_splits(&creditor_batch.proxy_txns);

    lm_debtor_client
        .update_split_only(debtor_repayment_txn.id, &debtor_splits)
        .await?;

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
    println!("txns after retain: {:?}", txns);

    // Find the first transaction on the repayment account that has this batch name and a negative amount remove it from the vec
    // There should only be one, we'll check the balance of the batch later
    let repayment_txn = match txns.iter().position(|t| {
        is_in_acct(t, config.creditor.repayment_account_id) && t.amount.value() < dec!(0)
    }) {
        Some(position) => txns.swap_remove(position),
        None => return Err("didn't find repayment transaction".into()),
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
    if let Some(position) = txns.iter().position(|t| {
        t.payee.contains(batch_name) && is_in_acct(t, config.debtor.repayment_account_id)
    }) {
        return Ok(txns.swap_remove(position));
    } else {
        return Err("didn't find debtor repayment transaction".into());
    }
}

fn get_debtor_splits(creditor_proxy_txns: &Vec<Transaction>) -> Vec<Split> {
    creditor_proxy_txns
        .iter()
        .map({
            |t| Split {
                amount: t.amount,
                payee: Some(t.payee.to_owned()),
                category_id: None,
                notes: None,
                date: Some(t.date),
            }
        })
        .collect()
}
