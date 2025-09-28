use crate::config;
use chrono::NaiveDate;
use display_json::DebugAsJson;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(DebugAsJson, Deserialize, Serialize)]
pub struct BatchMetadata {
    pub name: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub reconciled: bool,
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

pub fn all_metas(profile: &String) -> Result<Vec<BatchMetadata>, Box<dyn std::error::Error>> {
    let path = get_or_create_data_dir(profile)?;
    let dir = fs::read_dir(path)?;
    let mut parsed_metas: Vec<BatchMetadata> = Vec::new();
    for entry in dir {
        let path = match entry?.path().to_str() {
            Some(s) => s.to_string(),
            None => continue,
        };
        if !path.ends_with("json") {
            continue;
        }
        let file = fs::read_to_string(path)?;
        let parsed: BatchMetadata = serde_json::from_str(&file)?;
        parsed_metas.push(parsed);
    }
    return Ok(parsed_metas);
}

pub fn unreconciled_metas(
    profile: &String,
) -> Result<Vec<BatchMetadata>, Box<dyn std::error::Error>> {
    all_metas(profile)?
        .into_iter()
        .filter(|m| !(*m).reconciled)
        .map(|m| Ok(m))
        .collect()
}

pub fn metadata_for_batch(
    batch_name: &String,
    profile: &String,
) -> Result<BatchMetadata, Box<dyn std::error::Error>> {
    let filename = filename_for(batch_name, profile)?;
    let file = fs::read_to_string(&filename)
        .map_err(|e| format!("error reading metadata file {}, {}", filename.display(), e))?;
    let parsed: BatchMetadata = serde_json::from_str(&file)?;
    return Ok(parsed);
}

pub fn save_new_batch_metadata(
    batch_name: &String,
    start_date: NaiveDate,
    end_date: NaiveDate,
    profile: &String,
) -> Result<(), Box<dyn std::error::Error>> {
    let new_meta = BatchMetadata {
        name: batch_name.to_owned(),
        start_date: start_date,
        end_date: end_date,
        reconciled: false,
    };

    tracing::debug!(?new_meta, "saving new batch metadata");

    if config::is_dry_run() {
        return Ok(());
    }

    let json = serde_json::to_string_pretty(&new_meta)?;
    fs::write(filename_for(batch_name, profile)?, json)?;
    Ok(())
}

pub fn set_reconciled(
    batch_name: &String,
    reconciled: bool,
    profile: &String,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut metadata = metadata_for_batch(batch_name, profile)?;
    metadata.reconciled = reconciled;

    tracing::debug!(?metadata, "setting reconciled = true in batch metadata");

    if config::is_dry_run() {
        return Ok(());
    }

    let json = serde_json::to_string_pretty(&metadata)?;
    fs::write(filename_for(batch_name, profile)?, json)?;
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
