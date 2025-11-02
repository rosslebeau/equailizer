use crate::config::{self, *};
use crate::email;
use crate::lunch_money::api::update_transaction::{Action, TransactionUpdate};
use crate::lunch_money::model::transaction;
use crate::persist::Batch;
use crate::usd::USD;
use crate::{
    lunch_money, lunch_money::api::Client, lunch_money::api::update_transaction,
    lunch_money::api::update_transaction::Split, lunch_money::model::transaction::TransactionId,
    lunch_money::model::transaction::*, persist,
};
use chrono::NaiveDate;
use rand::random_bool;
use rust_decimal::prelude::*;
use uuid::{self, Uuid};

pub async fn create_batch(
    start_date: NaiveDate,
    end_date: NaiveDate,
    profile: &String,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    // get transactions from creditor
    if start_date.cmp(&end_date) == std::cmp::Ordering::Greater {
        return Err("start date cannot be after end date".into());
    }

    let batch_id = Uuid::new_v4().to_string();
    // let mut batch_total: USD = USD::new(dec!(0));

    let creditor_client = Client {
        auth_token: config.creditor.api_key.to_owned(),
    };
    let txns = creditor_client
        .get_transactions(start_date, end_date)
        .await?;

    // send transactions to be processed into a batch
    let processed = process_tags(txns);

    // create Actions for the processed results
    let actions = create_actions(processed, config.creditor.proxy_category_id);

    // execute the Actions

    for action in actions {
        creditor_client.update2(0, action).await?;
    }

    // Save batch to local data

    // configure/send email
    // Some(tags_by_removing_tag(&txn, config::TAG_BATCH_ADD.into()))

    return Ok(());
}

fn create_actions(processed_data: ProcessTagsOutput, proxy_category_id: u32) -> Vec<Action> {
    let add_actions: Vec<Action> =
        create_add_actions(processed_data.txns_to_add, proxy_category_id);

    let mut split_actions: Vec<Action> =
        create_split_actions(processed_data.txns_to_split, proxy_category_id);

    let mut actions = add_actions;
    actions.append(&mut split_actions);
    return actions;
}

fn create_add_actions(txns_to_add: Vec<Transaction>, proxy_category_id: u32) -> Vec<Action> {
    txns_to_add
        .into_iter()
        .map(|txn| {
            Action::Update(TransactionUpdate {
                payee: None,
                category_id: Some(proxy_category_id),
                notes: None,
                tags: Some(tag_names_removing(
                    txn.tags,
                    config::TAG_BATCH_ADD.to_string(),
                )),
                status: Some(TransactionStatus::Cleared),
            })
        })
        .collect()
}

fn create_split_actions(txns_to_split: Vec<Transaction>, proxy_category_id: u32) -> Vec<Action> {
    txns_to_split
        .into_iter()
        .map(|txn| {
            let (creditor_amt, debtor_amt) = txn.amount.random_rounded_even_split();
            let (creditor_split, debtor_split) =
                create_splits(creditor_amt, debtor_amt, proxy_category_id);
            Action::UpdateAndSplit(
                TransactionUpdate {
                    payee: None,
                    category_id: None,
                    notes: None,
                    tags: Some(tag_names_removing(
                        txn.tags,
                        config::TAG_BATCH_SPLIT.to_string(),
                    )),
                    status: Some(TransactionStatus::Cleared),
                },
                vec![creditor_split, debtor_split],
            )
        })
        .collect()
}

// Always round one up and one down, so the total stays the same
// Return the split values in a random order, so over time neither half is larger

fn create_splits(creditor_amt: USD, debtor_amt: USD, proxy_category: u32) -> (Split, Split) {
    let creditor_split = Split {
        amount: creditor_amt,
        payee: None,
        category_id: None,
        notes: None,
        date: None,
    };

    let debtor_split = Split {
        amount: debtor_amt,
        payee: None,
        category_id: Some(proxy_category),
        notes: None,
        date: None,
    };

    return (creditor_split, debtor_split);
}

fn tag_names_removing(tags: Vec<Tag>, name_to_remove: String) -> Vec<String> {
    tags.into_iter()
        .map(|tag| tag.name)
        .filter(|name| *name != name_to_remove)
        .collect()
}

pub enum Issue {
    AddTagHasChildren(TransactionId),
    SplitTagHasParent(TransactionId),
    SplitTagHasChildren(TransactionId),
}

struct ProcessTagsOutput {
    txns_to_add: Vec<Transaction>,
    txns_to_split: Vec<Transaction>,
    issues: Vec<Issue>,
}

fn process_tags(in_txns: Vec<Transaction>) -> ProcessTagsOutput {
    let span = tracing::info_span!("Processing Tags");
    let _enter = span.enter();
    tracing::debug!("Starting");

    let mut issues: Vec<Issue> = vec![];

    // Just ignore pending transactions
    let (txns_to_add, txns_to_split) = in_txns.into_iter().filter(|t| !t.is_pending).fold(
        (Vec::<Transaction>::new(), Vec::<Transaction>::new()),
        |(mut add, mut split), txn| {
            if txn
                .tag_names()
                .contains(&&config::TAG_BATCH_ADD.to_string())
            {
                add.push(txn);
            } else if txn
                .tag_names()
                .contains(&&config::TAG_BATCH_SPLIT.to_string())
            {
                split.push(txn);
            }
            return (add, split);
        },
    );

    tracing::debug!(
        txns_to_add = %txns_to_add.iter().map(|t| format!("id: {}, amount: {}, date: {}", t.id, t.amount, t.date)).collect::<Vec<_>>().join(",\n"),
        txns_to_split = %txns_to_split.iter().map(|t| format!("id: {}, amount: {}, date: {}", t.id, t.amount, t.date)).collect::<Vec<_>>().join(",\n"),
        "Tagged transactions identified");

    let (txns_to_add, mut new_issues) = filter_invalid_txns_to_add(txns_to_add);
    issues.append(&mut new_issues);

    let (txns_to_split, mut new_issues) = filter_invalid_txns_to_split(txns_to_split);
    issues.append(&mut new_issues);

    return ProcessTagsOutput {
        txns_to_add: txns_to_add,
        txns_to_split: txns_to_split,
        issues: issues,
    };
}

fn filter_invalid_txns_to_add(txns: Vec<Transaction>) -> (Vec<Transaction>, Vec<Issue>) {
    txns.into_iter().fold(
        (Vec::<Transaction>::new(), Vec::<Issue>::new()),
        |(mut valid, mut issues), txn| {
            if txn.has_children {
                // This is a parent txn that is already split. These are
                // not shown in Lunch Money and it is user error to tag them
                // for equailizer processing.
                tracing::debug!(
                    txn_id = txn.id,
                    "Found 'add' tag, but transaction has children"
                );
                issues.push(Issue::AddTagHasChildren(txn.id));
            } else {
                valid.push(txn);
            }

            return (valid, issues);
        },
    )
}

fn filter_invalid_txns_to_split(txns: Vec<Transaction>) -> (Vec<Transaction>, Vec<Issue>) {
    txns.into_iter().fold(
        (Vec::<Transaction>::new(), Vec::<Issue>::new()),
        |(mut valid, mut issues), txn| {
            if txn.has_children {
                // This is a parent txn that is already split. These are
                // not shown in Lunch Money and it is user error to tag them
                // for equailizer processing.
                tracing::debug!(
                    txn_id = txn.id,
                    "Found 'split' tag, but transaction has children"
                );
                issues.push(Issue::SplitTagHasChildren(txn.id));
            } else if txn.parent_id.is_some() {
                // This is a parent txn that is already split. These are
                // not shown in Lunch Money and it is user error to tag them
                // for equailizer processing.
                tracing::debug!(
                    txn_id = txn.id,
                    "Found 'split' tag, but transaction already has a parent"
                );
                issues.push(Issue::SplitTagHasParent(txn.id));
            } else {
                valid.push(txn);
            }

            return (valid, issues);
        },
    )
}
