use chrono::NaiveDate;
use equailizer::usd::USD;

use equailizer::lunch_money::model::transaction::{
    Tag, Transaction, TransactionId, TransactionStatus,
};

/// Create a minimal test transaction with sensible defaults.
/// Use the `with_*` methods to customize.
pub fn test_transaction(id: TransactionId, amount_cents: i64) -> Transaction {
    Transaction {
        id,
        date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
        payee: format!("Payee_{}", id),
        amount: USD::new_from_cents(amount_cents),
        plaid_account_id: None,
        category_id: None,
        category_name: None,
        tags: vec![],
        notes: None,
        status: TransactionStatus::Cleared,
        parent_id: None,
        has_children: false,
        is_pending: false,
    }
}

pub trait TransactionBuilder {
    fn with_date(self, year: i32, month: u32, day: u32) -> Self;
    fn with_payee(self, payee: &str) -> Self;
    fn with_tags(self, tags: Vec<(&str, u32)>) -> Self;
    fn with_account(self, account_id: u32) -> Self;
    fn with_category(self, id: u32, name: &str) -> Self;
    fn with_notes(self, notes: &str) -> Self;
    fn with_status(self, status: TransactionStatus) -> Self;
    fn with_parent(self, parent_id: u32) -> Self;
    fn with_children(self) -> Self;
    fn pending(self) -> Self;
}

impl TransactionBuilder for Transaction {
    fn with_date(mut self, year: i32, month: u32, day: u32) -> Self {
        self.date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
        self
    }

    fn with_payee(mut self, payee: &str) -> Self {
        self.payee = payee.to_string();
        self
    }

    fn with_tags(mut self, tags: Vec<(&str, u32)>) -> Self {
        self.tags = tags
            .into_iter()
            .map(|(name, id)| Tag {
                name: name.to_string(),
                id,
            })
            .collect();
        self
    }

    fn with_account(mut self, account_id: u32) -> Self {
        self.plaid_account_id = Some(account_id);
        self
    }

    fn with_category(mut self, id: u32, name: &str) -> Self {
        self.category_id = Some(id);
        self.category_name = Some(name.to_string());
        self
    }

    fn with_notes(mut self, notes: &str) -> Self {
        self.notes = Some(notes.to_string());
        self
    }

    fn with_status(mut self, status: TransactionStatus) -> Self {
        self.status = status;
        self
    }

    fn with_parent(mut self, parent_id: u32) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    fn with_children(mut self) -> Self {
        self.has_children = true;
        self
    }

    fn pending(mut self) -> Self {
        self.is_pending = true;
        self
    }
}
