use crate::{
    commands::create_batch::process_tags::ProcessTagsOutput,
    lunch_money::{
        api::update_transaction::{
            SplitUpdateItem, TransactionAndSplitUpdate, TransactionUpdate, TransactionUpdateItem,
        },
        model::transaction::{Tag, Transaction, TransactionStatus},
    },
    usd::USD,
};

pub fn create_updates(
    processed_data: ProcessTagsOutput,
    proxy_category_id: u32,
) -> (Vec<TransactionUpdate>, Vec<TransactionAndSplitUpdate>) {
    let add_updates: Vec<TransactionUpdate> = create_add_updates(
        processed_data.txns_to_add,
        proxy_category_id,
        processed_data.add_tag,
    );

    let split_updates: Vec<TransactionAndSplitUpdate> = create_split_updates(
        processed_data.txns_to_split,
        proxy_category_id,
        processed_data.split_tag,
    );

    return (add_updates, split_updates);
}

fn create_add_updates(
    txns_to_add: Vec<Transaction>,
    proxy_category_id: u32,
    add_tag: String,
) -> Vec<TransactionUpdate> {
    txns_to_add
        .into_iter()
        .map(|txn| {
            (
                txn.id,
                TransactionUpdateItem {
                    payee: None,
                    category_id: Some(proxy_category_id),
                    notes: None,
                    tags: Some(tag_names_removing(txn.tags, &add_tag)),
                    status: Some(TransactionStatus::Cleared),
                },
            )
        })
        .collect()
}

fn create_split_updates(
    txns_to_split: Vec<Transaction>,
    proxy_category_id: u32,
    split_tag: String,
) -> Vec<TransactionAndSplitUpdate> {
    txns_to_split
        .into_iter()
        .map(|txn| {
            let (creditor_amt, debtor_amt) = txn.amount.random_rounded_even_split();
            let (creditor_split, debtor_split) =
                create_splits(creditor_amt, debtor_amt, proxy_category_id);
            return (
                txn.id,
                TransactionUpdateItem {
                    payee: None,
                    category_id: None,
                    notes: None,
                    tags: Some(tag_names_removing(txn.tags, &split_tag)),
                    status: Some(TransactionStatus::Cleared),
                },
                vec![creditor_split, debtor_split],
            );
        })
        .collect()
}

fn create_splits(
    creditor_amt: USD,
    debtor_amt: USD,
    proxy_category: u32,
) -> (SplitUpdateItem, SplitUpdateItem) {
    let creditor_split = SplitUpdateItem {
        amount: creditor_amt,
        payee: None,
        category_id: None,
        notes: None,
        date: None,
    };

    let debtor_split = SplitUpdateItem {
        amount: debtor_amt,
        payee: None,
        category_id: Some(proxy_category),
        notes: None,
        date: None,
    };

    return (creditor_split, debtor_split);
}

fn tag_names_removing(tags: Vec<Tag>, name_to_remove: &String) -> Vec<String> {
    tags.into_iter()
        .map(|tag| tag.name)
        .filter(|name| name != name_to_remove)
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    #[test]
    fn create_actions() {
        let add_tag = "add-tag".to_string();
        let split_tag = "split-tag".to_string();

        let add_t1: Transaction = Transaction {
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

        let add_t2: Transaction = Transaction {
            id: 1025,
            date: NaiveDate::from_ymd_opt(2025, 10, 22).expect("NaiveDate creation failed"),
            payee: "More Tags".to_string(),
            amount: USD::new_from_cents(1299),
            plaid_account_id: None,
            category_id: Some(42),
            category_name: Some("Testing".to_string()),
            tags: vec![
                Tag {
                    name: add_tag.clone(),
                    id: 0,
                },
                Tag {
                    name: "external-tag".to_string(),
                    id: 1,
                },
            ],
            notes: None,
            status: TransactionStatus::Uncleared,
            parent_id: None,
            has_children: false,
            is_pending: false,
        };

        let split_t1: Transaction = Transaction {
            id: 1026,
            date: NaiveDate::from_ymd_opt(2025, 10, 23).expect("NaiveDate creation failed"),
            payee: "Split1".to_string(),
            amount: USD::new_from_cents(1500),
            plaid_account_id: None,
            category_id: Some(42),
            category_name: Some("Testing".to_string()),
            tags: vec![Tag {
                name: split_tag.clone(),
                id: 0,
            }],
            notes: None,
            status: TransactionStatus::Cleared,
            parent_id: None,
            has_children: false,
            is_pending: false,
        };

        let split_t2: Transaction = Transaction {
            id: 1027,
            date: NaiveDate::from_ymd_opt(2025, 10, 24).expect("NaiveDate creation failed"),
            payee: "More Tags".to_string(),
            amount: USD::new_from_cents(1200),
            plaid_account_id: None,
            category_id: Some(42),
            category_name: Some("Testing".to_string()),
            tags: vec![
                Tag {
                    name: split_tag.clone(),
                    id: 0,
                },
                Tag {
                    name: "external-tag".to_string(),
                    id: 1,
                },
            ],
            notes: None,
            status: TransactionStatus::Uncleared,
            parent_id: None,
            has_children: false,
            is_pending: false,
        };

        let proxy_category_id = 20;
        let (add_updates, split_updates) = super::create_updates(
            ProcessTagsOutput {
                add_tag: add_tag,
                split_tag: split_tag,
                txns_to_add: vec![add_t1, add_t2],
                txns_to_split: vec![split_t1, split_t2],
                issues: vec![],
            },
            proxy_category_id,
        );

        let assert_add_updates = vec![
            (
                1024,
                TransactionUpdateItem {
                    payee: None,
                    category_id: Some(proxy_category_id),
                    notes: None,
                    tags: Some(vec![]),
                    status: Some(TransactionStatus::Cleared),
                },
            ),
            (
                1025,
                TransactionUpdateItem {
                    payee: None,
                    category_id: Some(proxy_category_id),
                    notes: None,
                    tags: Some(vec!["external-tag".to_string()]),
                    status: Some(TransactionStatus::Cleared),
                },
            ),
        ];

        let assert_split_updates = vec![
            (
                1026,
                TransactionUpdateItem {
                    payee: None,
                    category_id: None,
                    notes: None,
                    tags: Some(vec![]),
                    status: Some(TransactionStatus::Cleared),
                },
                vec![
                    SplitUpdateItem {
                        amount: USD::new_from_cents(750),
                        payee: None,
                        category_id: None,
                        notes: None,
                        date: None,
                    },
                    SplitUpdateItem {
                        amount: USD::new_from_cents(750),
                        payee: None,
                        category_id: Some(proxy_category_id),
                        notes: None,
                        date: None,
                    },
                ],
            ),
            (
                1027,
                TransactionUpdateItem {
                    payee: None,
                    category_id: None,
                    notes: None,
                    tags: Some(vec!["external-tag".to_string()]),
                    status: Some(TransactionStatus::Cleared),
                },
                vec![
                    SplitUpdateItem {
                        amount: USD::new_from_cents(600),
                        payee: None,
                        category_id: None,
                        notes: None,
                        date: None,
                    },
                    SplitUpdateItem {
                        amount: USD::new_from_cents(600),
                        payee: None,
                        category_id: Some(proxy_category_id),
                        notes: None,
                        date: None,
                    },
                ],
            ),
        ];

        assert_eq!(add_updates, assert_add_updates);
        assert_eq!(split_updates, assert_split_updates);
    }
}
