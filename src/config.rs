use serde::Deserialize;
use std::fs;
use uuid::Uuid;

pub const TAG_BATCH_SPLIT: &str = "eq-to-split";
pub const TAG_BATCH_ADD: &str = "eq-to-batch";

pub fn eq_batch_name(from_uuid: Uuid) -> String {
    format!("eq<{from_uuid}>")
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub creditor: Creditor,
    pub debtor: Debtor,
}

#[derive(Debug, Deserialize)]
pub struct Creditor {
    pub api_key: String,
    pub proxy_category_id: u32,
    pub repayment_account_id: u32,
}

#[derive(Debug, Deserialize)]
pub struct Debtor {
    pub api_key: String,
    pub repayment_account_id: u32,
}

pub fn read_config() -> Result<Config, Box<dyn std::error::Error>> {
    let file = fs::read_to_string("eq-config.json").expect("eq-config.json should be present");
    let parsed: Config = serde_json::from_str(&file)?;
    return Ok(parsed);
}
