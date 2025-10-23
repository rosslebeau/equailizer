mod cli;
mod commands;
mod config;
mod date_helpers;
mod email;
mod log;
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
    let log_guard = log::init_tracing();

    if config::is_dry_run() {
        tracing::info!("dry run beginning");
    }

    let args = cli::Equailizer::parse();

    match args.command {
        cli::Commands::CreateBatch {
            start,
            end_date,
            profile,
            dry_run,
        } => match handle_create_batch(start, end_date, profile, dry_run).await {
            Ok(output) => tracing::info!(output),
            Err(e) => tracing::error!(e, "creating batch failed"),
        },
        cli::Commands::Reconcile {
            batch_name,
            profile,
            dry_run,
        } => match handle_reconcile(batch_name, profile, dry_run).await {
            Ok(output) => tracing::info!(output),
            Err(e) => tracing::error!(e, "reconciling batch failed"),
        },
        cli::Commands::ReconcileAll { profile, dry_run } => {
            match handle_reconcile_all(profile, dry_run).await {
                Ok(()) => tracing::info!("Successfully reconciled all outstanding batches"),
                Err(e) => tracing::error!(e, "reconciling failed"),
            }
        }
    }

    if config::is_dry_run() {
        tracing::info!("dry run ended");
    }

    drop(log_guard);
}

async fn handle_create_batch(
    start: StartArgs,
    end_date: Option<NaiveDate>,
    profile: String,
    dry_run: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    config::set_dry_run(dry_run);
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
    profile: String,
    dry_run: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    config::set_dry_run(dry_run);
    let config = config::read_config(&profile)?;
    commands::reconcile::reconcile_batch_name(&batch_name, &config, &profile).await?;
    return Ok(format!("Successfully reconciled batch: {}", batch_name));
}

async fn handle_reconcile_all(
    profile: String,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    config::set_dry_run(dry_run);
    let config = config::read_config(&profile)?;
    commands::reconcile::reconcile_all(&config, &profile).await?;
    return Ok(());
}
