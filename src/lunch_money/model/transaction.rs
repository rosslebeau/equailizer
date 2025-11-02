use crate::usd::USD;
use chrono::NaiveDate;
use display_json::DebugAsJson;
use serde::{Deserialize, Serialize};

pub type TransactionId = u32;

#[derive(Deserialize, Serialize, DebugAsJson, PartialEq, Clone)]
pub struct Transaction {
    pub id: TransactionId,
    pub date: NaiveDate,
    pub payee: String,
    pub amount: USD, // All my accounts are in dollars. No need for currency complexity just yet.
    pub plaid_account_id: Option<u32>,
    pub category_id: Option<u32>,
    pub category_name: Option<String>,
    pub tags: Vec<Tag>,
    pub notes: Option<String>,
    pub status: TransactionStatus,
    pub parent_id: Option<u32>,
    pub has_children: bool,
    pub is_pending: bool,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct Tag {
    pub name: String,
    pub id: u32,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub enum TransactionStatus {
    #[serde(rename = "cleared")]
    Cleared,

    #[serde(rename = "uncleared")]
    Uncleared,

    #[serde(rename = "pending")]
    Pending,

    #[serde(rename = "delete_pending")]
    DeletePending,
}

impl Transaction {
    pub fn tag_names(&self) -> Vec<&String> {
        self.tags.iter().map(|x| &x.name).collect()
    }
}

// #[cfg(test)]
// mod tests {
//     use chrono::NaiveDate;
//     use rust_decimal::dec;

//     use crate::{usd::USD};

//     use super::*;

//     #[test]
//     fn tag_names() {
//         let txn = Transaction {
//             id: 1,
//             date: NaiveDate::new(),
//             payee: "Test".to_string(),
//             amount: USD(dec!(0)),

//         }
//     }
// }
