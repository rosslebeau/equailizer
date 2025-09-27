mod cli;
mod commands;
mod config;
mod date_helpers;
mod email;
mod lunch_money;
mod persist;
pub mod usd;

use chrono::NaiveDate;
use clap::Parser;

use crate::cli::StartArgs;
use core::result::Result;
use date_helpers::*;

#[tokio::main]
async fn main() {
    let args = cli::Equailizer::parse();

    match args.command {
        cli::Commands::CreateBatch {
            start,
            end_date,
            profile,
        } => match handle_create_batch(start, end_date, profile).await {
            Ok(output) => println!("{}", output),
            Err(e) => println!("Creating batch failed with error: {}", e),
        },
        cli::Commands::Reconcile {
            batch_name,
            start,
            end_date,
            profile,
        } => match handle_reconcile(batch_name, start, end_date, profile).await {
            Ok(output) => println!("{}", output),
            Err(e) => println!("Reconciling batch failed with error: {}", e),
        },
        cli::Commands::ReconcileAll { profile } => match handle_reconcile_all(profile).await {
            Ok(output) => println!("{}", output),
            Err(e) => println!("Reconciling batch failed with error: {}", e),
        },
    }
}

async fn handle_create_batch(
    start: StartArgs,
    end_date: Option<NaiveDate>,
    profile: String,
) -> Result<String, Box<dyn std::error::Error>> {
    let config = config::read_config(&profile)?;
    let start_date = cli::start_date_from_args(start);
    let end_date = end_date.or_naive_date_now();
    let result = commands::create_batch::run(start_date, end_date, &config, &profile).await?;
    return Ok(format!(
        "Successfully created batch!\nBatch label: {}\nBatch amount: {}",
        result.batch_label, result.batch_amount
    ));
}

async fn handle_reconcile(
    batch_name: String,
    start: StartArgs,
    end_date: Option<NaiveDate>,
    profile: String,
) -> Result<String, Box<dyn std::error::Error>> {
    let config = config::read_config(&profile)?;
    let start_date = cli::start_date_from_args(start);
    let end_date = end_date.or_naive_date_now();
    commands::reconcile::reconcile_batch(&batch_name, start_date, end_date, &config, &profile)
        .await?;
    return Ok(format!("Successfully reconciled batch: {}", batch_name));
}

async fn handle_reconcile_all(profile: String) -> Result<String, Box<dyn std::error::Error>> {
    let config = config::read_config(&profile)?;
    let reconciled_batch_names = commands::reconcile::reconcile_all(&config, &profile).await?;
    return Ok(format!(
        "Successfully reconciled all outstanding batches:\n{}",
        reconciled_batch_names.join("\n")
    ));
}
