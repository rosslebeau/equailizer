use anyhow::Result;
use serde::Deserialize;
use std::fs;

use crate::persist;

pub const TAG_BATCH_SPLIT: &str = "eq-to-split";
pub const TAG_BATCH_ADD: &str = "eq-to-batch";

#[derive(Debug, Deserialize)]
pub struct Config {
    pub creditor: Creditor,
    pub debtor: Debtor,
    pub jmap: JMAP,
}

#[derive(Debug, Deserialize)]
pub struct Creditor {
    pub api_key: String,
    pub proxy_category_id: u32,
    pub settlement_account_id: u32,
    pub email_address: String,
}

#[derive(Debug, Deserialize)]
pub struct Debtor {
    pub api_key: String,
    pub name: String,
    pub settlement_account_id: u32,
    pub email_address: String,
    pub venmo_username: String,
}

#[derive(Debug, Deserialize)]
pub struct JMAP {
    pub api_session_endpoint: String,
    pub api_key: String,
    pub sent_mailbox: String,
    pub sending_address: String,
}

pub fn read_config(profile: &str) -> Result<Config> {
    let mut config_path = persist::base_path()?;
    config_path.push(format!("profiles/{}/config.json", profile));

    let file = fs::read_to_string(config_path).expect("config.json should be present");
    let parsed: Config = serde_json::from_str(&file)?;
    Ok(parsed)
}
