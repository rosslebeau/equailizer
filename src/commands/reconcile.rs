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
    let lm_creditor_client = crate::lunch_money::api::Client {
        auth_token: config.creditor.api_key.to_owned(),
    };

    let mut creditor_batch_txns: Vec<Transaction> = Vec::new();
    for txn_id in &batch.transaction_ids {
        creditor_batch_txns.push(lm_creditor_client.get_transaction(*txn_id).await?);
    }

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

    // if (batch_total + creditor_batch.repayment_txn.amount) != USD::new(dec!(0)) {
    //     return Err("batch total is not equal to repayment transaction".into());
    // }

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

    // let debtor_repayment_txn =
    //     get_debtor_repayment_txn_from_txns(debtor_txns, &batch.name, config)?;

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

    tracing::info!(updated_batch.name, "Reconciled batch");

    return Ok(());
}

fn is_in_acct(txn: &Transaction, account_id: u32) -> bool {
    match txn.plaid_account_id {
        Some(acct_id) => acct_id == account_id,
        None => false,
    }
}

// async fn get_creditor_batch_from_txns(
//     mut txns: Vec<Transaction>,
//     batch_name: &String,
//     config: &Config,
// ) -> Result<CreditorBatch, Box<dyn std::error::Error>> {
//     // Look for transactions that have the batch name in either:
//     // - the payee (repayment txn), or
//     // - the notes (previously batched proxy txns)
//     txns.retain(|t| {
//         t.payee.contains(batch_name) || t.notes.as_ref().is_some_and(|n| n.contains(batch_name))
//     });

//     // update to above. new logic should be:
//     // take in a list of all txn ids in the batch, pull those out
//     // look for creditor's side of the repayment: find the txn with a negative amount (thus income) equal to the batch amount

//     // Find the first transaction on the repayment account that has this batch name and a negative amount remove it from the vec
//     // There should only be one, we'll check the balance of the batch later
//     let repayment_txn = match txns.iter().position(|t| {
//         is_in_acct(t, config.creditor.repayment_account_id) && t.amount.value() < dec!(0)
//     }) {
//         Some(position) => txns.swap_remove(position),
//         None => return Err("didn't find creditor repayment transaction".into()),
//     };

//     return Ok(CreditorBatch {
//         repayment_txn: repayment_txn,
//         proxy_txns: txns,
//     });
// }

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
