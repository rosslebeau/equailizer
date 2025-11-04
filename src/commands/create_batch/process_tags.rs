use crate::lunch_money::model::transaction::{Transaction, TransactionId};

#[derive(Debug, PartialEq)]
pub struct ProcessTagsOutput {
    pub add_tag: String,
    pub split_tag: String,
    pub txns_to_add: Vec<Transaction>,
    pub txns_to_split: Vec<Transaction>,
    pub issues: Vec<Issue>,
}

#[derive(Debug, PartialEq)]
pub enum Issue {
    AddTagHasChildren(TransactionId),
    SplitTagHasParent(TransactionId),
    SplitTagHasChildren(TransactionId),
}

pub fn process_tags(
    in_txns: Vec<Transaction>,
    add_tag: &String,
    split_tag: &String,
) -> ProcessTagsOutput {
    let span = tracing::info_span!("Processing Tags");
    let _enter = span.enter();
    tracing::debug!("Starting");

    let mut issues: Vec<Issue> = vec![];

    // Just ignore pending transactions
    let (txns_to_add, txns_to_split) = in_txns.into_iter().filter(|t| !t.is_pending).fold(
        (Vec::<Transaction>::new(), Vec::<Transaction>::new()),
        |(mut add, mut split), txn| {
            if txn.tag_names().contains(&&add_tag) {
                add.push(txn);
            } else if txn.tag_names().contains(&&split_tag) {
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
        add_tag: add_tag.clone(),
        split_tag: split_tag.clone(),
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

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use crate::{
        lunch_money::model::transaction::{Tag, TransactionStatus},
        usd::USD,
    };

    use super::*;

    #[test]
    fn process_tags() {
        let add_tag = "add-tag".to_string();
        let split_tag = "split-tag".to_string();

        let add_t: Transaction = Transaction {
            id: 1024,
            date: NaiveDate::from_ymd_opt(2025, 10, 21).expect("NaiveDate creation failed"),
            payee: "JetBlue".to_string(),
            amount: USD::new_from_cents(18522),
            plaid_account_id: None,
            category_id: Some(41),
            category_name: Some("Airfaire".to_string()),
            tags: vec![Tag {
                name: add_tag.clone(),
                id: 0,
            }],
            notes: Some("Ticket to Chicago".to_string()),
            status: TransactionStatus::Uncleared,
            parent_id: None,
            has_children: false,
            is_pending: false,
        };

        let add_has_children_t: Transaction = Transaction {
            id: 1025,
            date: NaiveDate::from_ymd_opt(2025, 10, 22).expect("NaiveDate creation failed"),
            payee: "Has Children".to_string(),
            amount: USD::new_from_cents(1299),
            plaid_account_id: None,
            category_id: Some(42),
            category_name: Some("Testing".to_string()),
            tags: vec![Tag {
                name: add_tag.clone(),
                id: 0,
            }],
            notes: None,
            status: TransactionStatus::Uncleared,
            parent_id: None,
            has_children: true,
            is_pending: false,
        };

        let split_t: Transaction = Transaction {
            id: 1026,
            date: NaiveDate::from_ymd_opt(2025, 10, 22).expect("NaiveDate creation failed"),
            payee: "Split".to_string(),
            amount: USD::new_from_cents(1299),
            plaid_account_id: None,
            category_id: Some(42),
            category_name: Some("Testing".to_string()),
            tags: vec![Tag {
                name: split_tag.clone(),
                id: 0,
            }],
            notes: None,
            status: TransactionStatus::Uncleared,
            parent_id: None,
            has_children: false,
            is_pending: false,
        };

        let split_has_parent_t: Transaction = Transaction {
            id: 1027,
            date: NaiveDate::from_ymd_opt(2025, 10, 22).expect("NaiveDate creation failed"),
            payee: "Split".to_string(),
            amount: USD::new_from_cents(1299),
            plaid_account_id: None,
            category_id: Some(42),
            category_name: Some("Testing".to_string()),
            tags: vec![Tag {
                name: split_tag.clone(),
                id: 0,
            }],
            notes: None,
            status: TransactionStatus::Uncleared,
            parent_id: Some(1),
            has_children: false,
            is_pending: false,
        };

        let split_has_children_t: Transaction = Transaction {
            id: 1028,
            date: NaiveDate::from_ymd_opt(2025, 10, 22).expect("NaiveDate creation failed"),
            payee: "Split".to_string(),
            amount: USD::new_from_cents(1299),
            plaid_account_id: None,
            category_id: Some(42),
            category_name: Some("Testing".to_string()),
            tags: vec![Tag {
                name: split_tag.clone(),
                id: 0,
            }],
            notes: None,
            status: TransactionStatus::Uncleared,
            parent_id: None,
            has_children: true,
            is_pending: false,
        };

        let output = super::process_tags(
            vec![
                add_t.clone(),
                add_has_children_t,
                split_t.clone(),
                split_has_parent_t,
                split_has_children_t,
            ],
            &add_tag,
            &split_tag,
        );

        let add_has_children_issue: Issue = Issue::AddTagHasChildren(1025);
        let split_has_parent_issue: Issue = Issue::SplitTagHasParent(1027);
        let split_has_children_issue: Issue = Issue::SplitTagHasChildren(1028);

        let assert_output: ProcessTagsOutput = ProcessTagsOutput {
            add_tag,
            split_tag,
            txns_to_add: vec![add_t],
            txns_to_split: vec![split_t],
            issues: vec![
                add_has_children_issue,
                split_has_parent_issue,
                split_has_children_issue,
            ],
        };

        assert_eq!(output, assert_output);
    }
}
