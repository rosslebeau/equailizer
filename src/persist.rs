use crate::{config, lunch_money::model::transaction::TransactionId, usd::USD};
use display_json::DebugAsJson;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(DebugAsJson, Deserialize, Serialize)]
pub struct Batch {
    pub id: String,
    pub amount: USD,
    pub transaction_ids: Vec<TransactionId>,
    pub reconciliation: Option<Settlement>,
}

#[derive(DebugAsJson, Deserialize, Serialize)]
pub struct Settlement {
    pub settlement_credit_id: TransactionId,
    pub settlement_debit_id: TransactionId,
}

pub fn base_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut base_path = std::env::current_exe()?
        .parent()
        .ok_or(Box::<dyn std::error::Error>::from(
            "could not open path of executable",
        ))?
        .to_path_buf();

    if cfg!(debug_assertions) {
        base_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    }

    return Ok(base_path);
}

pub fn all_batches(profile: &String) -> Result<Vec<Batch>, Box<dyn std::error::Error>> {
    let path = get_or_create_data_dir(profile)?;
    let dir = fs::read_dir(path)?;
    let mut parsed_metas: Vec<Batch> = Vec::new();
    for entry in dir {
        let path = match entry?.path().to_str() {
            Some(s) => s.to_string(),
            None => continue,
        };
        if !path.ends_with("json") {
            continue;
        }
        let file = fs::read_to_string(path)?;
        let parsed: Batch = serde_json::from_str(&file)?;
        parsed_metas.push(parsed);
    }
    return Ok(parsed_metas);
}

pub fn unreconciled_batches(profile: &String) -> Result<Vec<Batch>, Box<dyn std::error::Error>> {
    all_batches(profile)?
        .into_iter()
        .filter(|m| (*m).reconciliation.is_none())
        .map(|m| Ok(m))
        .collect()
}

pub fn get_batch(
    batch_name: &String,
    profile: &String,
) -> Result<Batch, Box<dyn std::error::Error>> {
    let filename = filename_for(batch_name, profile)?;
    let file = fs::read_to_string(&filename)
        .map_err(|e| format!("error reading batch file {}, {}", filename.display(), e))?;
    let parsed: Batch = serde_json::from_str(&file)?;
    return Ok(parsed);
}

pub fn save_batch(batch: &Batch, profile: &String) -> Result<(), Box<dyn std::error::Error>> {
    tracing::debug!(?batch, "saving batch");

    if config::is_dry_run() {
        return Ok(());
    }

    let json = serde_json::to_string_pretty(batch)?;
    fs::write(filename_for(&batch.id, profile)?, json)?;
    Ok(())
}

fn filename_for(
    batch_name: &String,
    profile: &String,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = get_or_create_data_dir(profile)?;
    let file_path = path.join(format!("{}.json", batch_name));
    Ok(file_path)
}

fn get_or_create_data_dir(profile: &String) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut data_path = base_path()?;
    data_path.push(format!("profiles/{}/data", profile));

    if !data_path.is_dir() {
        if data_path.exists() {
            return Err(
                "cannot create or open data directory - non-directory file exists at this path"
                    .into(),
            );
        } else {
            fs::create_dir_all(data_path.as_path())?;
        }
    }
    return Ok(data_path);
}
