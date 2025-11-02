use crate::{
    commands::c_b::process_tags::ProcessTagsOutput,
    lunch_money::{
        api::update_transaction::{Action, Split, TransactionUpdate},
        model::transaction::{Tag, Transaction, TransactionStatus},
    },
    usd::USD,
};

pub fn create_actions(processed_data: ProcessTagsOutput, proxy_category_id: u32) -> Vec<Action> {
    let add_actions: Vec<Action> = create_add_actions(
        processed_data.txns_to_add,
        proxy_category_id,
        processed_data.add_tag,
    );

    let mut split_actions: Vec<Action> = create_split_actions(
        processed_data.txns_to_split,
        proxy_category_id,
        processed_data.split_tag,
    );

    let mut actions = add_actions;
    actions.append(&mut split_actions);
    return actions;
}

fn create_add_actions(
    txns_to_add: Vec<Transaction>,
    proxy_category_id: u32,
    add_tag: String,
) -> Vec<Action> {
    txns_to_add
        .into_iter()
        .map(|txn| {
            Action::Update(TransactionUpdate {
                payee: None,
                category_id: Some(proxy_category_id),
                notes: None,
                tags: Some(tag_names_removing(txn.tags, &add_tag)),
                status: Some(TransactionStatus::Cleared),
            })
        })
        .collect()
}

fn create_split_actions(
    txns_to_split: Vec<Transaction>,
    proxy_category_id: u32,
    split_tag: String,
) -> Vec<Action> {
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
                    tags: Some(tag_names_removing(txn.tags, &split_tag)),
                    status: Some(TransactionStatus::Cleared),
                },
                vec![creditor_split, debtor_split],
            )
        })
        .collect()
}

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
        let actions = super::create_actions(
            ProcessTagsOutput {
                add_tag: add_tag,
                split_tag: split_tag,
                txns_to_add: vec![add_t1, add_t2],
                txns_to_split: vec![split_t1, split_t2],
                issues: vec![],
            },
            proxy_category_id,
        );

        let assert_actions = vec![
            Action::Update(TransactionUpdate {
                payee: None,
                category_id: Some(proxy_category_id),
                notes: None,
                tags: Some(vec![]),
                status: Some(TransactionStatus::Cleared),
            }),
            Action::Update(TransactionUpdate {
                payee: None,
                category_id: Some(proxy_category_id),
                notes: None,
                tags: Some(vec!["external-tag".to_string()]),
                status: Some(TransactionStatus::Cleared),
            }),
            Action::UpdateAndSplit(
                TransactionUpdate {
                    payee: None,
                    category_id: None,
                    notes: None,
                    tags: Some(vec![]),
                    status: Some(TransactionStatus::Cleared),
                },
                vec![
                    Split {
                        amount: USD::new_from_cents(750),
                        payee: None,
                        category_id: None,
                        notes: None,
                        date: None,
                    },
                    Split {
                        amount: USD::new_from_cents(750),
                        payee: None,
                        category_id: Some(proxy_category_id),
                        notes: None,
                        date: None,
                    },
                ],
            ),
            Action::UpdateAndSplit(
                TransactionUpdate {
                    payee: None,
                    category_id: None,
                    notes: None,
                    tags: Some(vec!["external-tag".to_string()]),
                    status: Some(TransactionStatus::Cleared),
                },
                vec![
                    Split {
                        amount: USD::new_from_cents(600),
                        payee: None,
                        category_id: None,
                        notes: None,
                        date: None,
                    },
                    Split {
                        amount: USD::new_from_cents(600),
                        payee: None,
                        category_id: Some(proxy_category_id),
                        notes: None,
                        date: None,
                    },
                ],
            ),
        ];

        assert_eq!(actions, assert_actions);
    }
}
