use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Deserialize, Serialize)]
pub struct BatchMetadata {
    pub name: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub reconciled: bool,
}

pub fn all_metas() -> Result<Vec<BatchMetadata>, Box<dyn std::error::Error>> {
    let dir = fs::read_dir("data")?;
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

pub fn unreconciled_metas() -> Result<Vec<BatchMetadata>, Box<dyn std::error::Error>> {
    all_metas()?
        .into_iter()
        .filter(|m| !(*m).reconciled)
        .map(|m| Ok(m))
        .collect()
}

pub fn metadata_for_batch(
    batch_name: &String,
) -> Result<BatchMetadata, Box<dyn std::error::Error>> {
    let filename = filename_for(batch_name);
    let file = fs::read_to_string(&filename)
        .map_err(|e| format!("error reading metadata file {}, {}", &filename, e))?;
    let parsed: BatchMetadata = serde_json::from_str(&file)?;
    return Ok(parsed);
}

pub fn save_new_batch_metadata(
    batch_name: &String,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<(), Box<dyn std::error::Error>> {
    let new_meta = BatchMetadata {
        name: batch_name.to_owned(),
        start_date: start_date,
        end_date: end_date,
        reconciled: false,
    };
    let json = serde_json::to_string_pretty(&new_meta)?;
    fs::write(filename_for(batch_name), json)?;
    Ok(())
}

pub fn set_reconciled(
    batch_name: &String,
    reconciled: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut metadata = metadata_for_batch(batch_name)?;
    metadata.reconciled = reconciled;
    let json = serde_json::to_string_pretty(&metadata)?;
    fs::write(filename_for(batch_name), json)?;
    Ok(())
}

fn filename_for(batch_name: &String) -> String {
    format!("data/{}.json", batch_name)
}
