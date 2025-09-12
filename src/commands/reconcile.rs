use crate::config::{self, Config, eq_batch_name};
use crate::lunch_money::api::update_transaction::{Split, TransactionUpdate};
use crate::lunch_money::model::transaction::{self, *};
use chrono::NaiveDate;
use rust_decimal::prelude::*;
use uuid::{self, Uuid};

pub async fn run(
    config: &Config,
    batch_id: Uuid,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<(), Box<dyn std::error::Error>> {
    let creditor_data = get_creditor_txns(config, batch_id, start_date, end_date).await?;

    let batch_total = creditor_data
        .proxy_txns
        .iter()
        .map(|t| t.amount)
        .fold(USD::new(dec!(0)), |acc, amt| acc + amt);

    if (batch_total + creditor_data.repayment_txn.amount) != USD::new(dec!(0)) {
        return Err("batch total is not equal to repayment transaction".into());
    }

    let lm_creditor_client = crate::lunch_money::api::Client {
        auth_token: config.creditor.api_key.to_owned(),
    };

    let txn_update = TransactionUpdate {
        payee: None,
        notes: None,
        tags: None,
        status: Some(TransactionStatus::Cleared),
    };
    lm_creditor_client.update_txn_only(creditor_data.repayment_txn.id, txn_update);

    let debtor_repayment_txn =
        get_debtor_repayment_txn(config, batch_id, start_date, end_date).await?;

    // create splits on debtor's side to pass through the payees so they can categorize
    let debtor_splits = creditor_data.proxy_txns.iter().map({
        |t| Split {
            amount: t.amount,
            payee: Some(t.payee.to_owned()),
            category_id: None,
            notes: None,
        }
    });

    let lm_creditor_client = crate::lunch_money::api::Client {
        auth_token: config.creditor.api_key.to_owned(),
    };

    return Ok(());
}

fn is_in_acct(txn: &Transaction, account_id: u32) -> bool {
    match txn.plaid_account_id {
        Some(acct_id) => acct_id == account_id,
        None => false,
    }
}

struct CreditorTransactions {
    repayment_txn: Transaction,
    proxy_txns: Vec<Transaction>,
}

async fn get_creditor_txns(
    config: &Config,
    batch_id: Uuid,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<CreditorTransactions, Box<dyn std::error::Error>> {
    let batch_name = eq_batch_name(batch_id);
    let lm_creditor_client = crate::lunch_money::api::Client {
        auth_token: config.creditor.api_key.to_owned(),
    };
    let mut creditor_txns = lm_creditor_client
        .get_transactions(start_date, end_date)
        .await?;

    creditor_txns.retain(|t| t.payee.contains(&batch_name));

    // Find the first transaction on the repayment account that has this batch name and remove it from the vec
    // There should only be one, we'll check the balance of the batch later
    let repayment_txn = match creditor_txns
        .iter()
        .position(|t| is_in_acct(t, config.creditor.repayment_account_id))
    {
        Some(position) => creditor_txns.swap_remove(position),
        None => return Err("didn't find repayment transaction".into()),
    };

    return Ok(CreditorTransactions {
        repayment_txn: repayment_txn,
        proxy_txns: creditor_txns,
    });
}

async fn get_debtor_repayment_txn(
    config: &Config,
    batch_id: Uuid,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<Transaction, Box<dyn std::error::Error>> {
    let batch_name = eq_batch_name(batch_id);
    let lm_creditor_client = crate::lunch_money::api::Client {
        auth_token: config.creditor.api_key.to_owned(),
    };
    let mut debtor_txns = lm_creditor_client
        .get_transactions(start_date, end_date)
        .await?;

    if let Some(position) = debtor_txns.iter().position(|t| {
        t.payee.contains(&batch_name) && is_in_acct(t, config.debtor.repayment_account_id)
    }) {
        return Ok(debtor_txns.remove(position));
    } else {
        return Err("didn't find debtor repayment transaction".into());
    }
}
