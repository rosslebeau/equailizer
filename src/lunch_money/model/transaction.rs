use crate::usd::USD;
use chrono::NaiveDate;
use display_json::DebugAsJson;
use serde::{Deserialize, Serialize};

pub type Id = u32;

#[derive(Deserialize, Serialize, DebugAsJson)]
pub struct Transaction {
    pub id: Id,
    pub date: NaiveDate,
    pub payee: String,
    pub amount: USD, // All my accounts are in dollars. No need for currency complexity just yet.
    pub plaid_account_id: Option<u32>,
    pub category_id: Option<u32>,
    pub category_name: Option<String>,
    pub tags: Vec<Tag>,
    pub notes: Option<String>,
    pub status: TransactionStatus,
    pub original_name: Option<String>,
    pub has_children: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Tag {
    pub name: String,
    pub id: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum TransactionStatus {
    #[serde(rename = "cleared")]
    Cleared,

    #[serde(rename = "uncleared")]
    Uncleared,

    #[serde(rename = "pending")]
    Pending,
}
