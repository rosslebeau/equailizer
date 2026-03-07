use crate::lunch_money::model::transaction::TransactionId;
use crate::usd::USD;
use anyhow::{Result, bail};
use display_json::DebugAsJson;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(DebugAsJson, Deserialize, Serialize, Clone)]
pub struct Batch {
    pub id: String,
    pub amount: USD,
    pub transaction_ids: Vec<TransactionId>,
    pub reconciliation: Option<Settlement>,
}

#[derive(DebugAsJson, Deserialize, Serialize, Clone)]
pub struct Settlement {
    pub settlement_credit_id: TransactionId,
    pub settlement_debit_id: TransactionId,
}

pub trait Persistence {
    fn save_batch(&self, batch: &Batch) -> Result<()>;
    fn get_batch(&self, batch_name: &str) -> Result<Batch>;
    fn all_batches(&self) -> Result<Vec<Batch>>;
    fn unreconciled_batches(&self) -> Result<Vec<Batch>>;
}

pub struct FilePersistence {
    data_path: PathBuf,
    dry_run: bool,
}

impl FilePersistence {
    pub fn new(profile: &str, dry_run: bool) -> Result<Self> {
        let mut data_path = base_path()?;
        data_path.push(format!("profiles/{}/data", profile));

        if !data_path.is_dir() {
            if data_path.exists() {
                bail!("cannot create data directory - non-directory file exists at this path");
            } else {
                fs::create_dir_all(data_path.as_path())?;
            }
        }

        Ok(Self { data_path, dry_run })
    }
}

impl Persistence for FilePersistence {
    fn save_batch(&self, batch: &Batch) -> Result<()> {
        tracing::debug!(?batch, "saving batch");

        if self.dry_run {
            return Ok(());
        }

        let json = serde_json::to_string_pretty(batch)?;
        let file_path = self.data_path.join(format!("{}.json", batch.id));
        fs::write(file_path, json)?;
        Ok(())
    }

    fn get_batch(&self, batch_name: &str) -> Result<Batch> {
        let file_path = self.data_path.join(format!("{}.json", batch_name));
        let file = fs::read_to_string(&file_path)
            .map_err(|e| anyhow::anyhow!("error reading batch file {}, {}", file_path.display(), e))?;
        let parsed: Batch = serde_json::from_str(&file)?;
        Ok(parsed)
    }

    fn all_batches(&self) -> Result<Vec<Batch>> {
        let dir = fs::read_dir(&self.data_path)?;
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
        Ok(parsed_metas)
    }

    fn unreconciled_batches(&self) -> Result<Vec<Batch>> {
        Ok(self
            .all_batches()?
            .into_iter()
            .filter(|m| m.reconciliation.is_none())
            .collect())
    }
}

pub fn base_path() -> Result<PathBuf> {
    let mut base_path = std::env::current_exe()?
        .parent()
        .ok_or_else(|| anyhow::anyhow!("could not open path of executable"))?
        .to_path_buf();

    if cfg!(debug_assertions) {
        base_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    }

    Ok(base_path)
}
