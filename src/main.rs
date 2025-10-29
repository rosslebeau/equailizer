#![recursion_limit = "512"]

mod cli;
mod commands;
mod config;
mod date_helpers;
mod email;
mod log;
mod lunch_money;
mod persist;
pub mod usd;

use crate::usd::USD;
use chrono::NaiveDate;
use clap::Parser;
use rust_decimal::dec;

use crate::{cli::StartArgs, email::Txn};
use core::result::Result;
use date_helpers::*;

#[tokio::main]
async fn main() {
    let log_guard = log::init_tracing();

    let html_string = email::make_html_string(
        vec![
            Txn {
                payee: "Associated Market".to_string(),
                amount: USD::new(dec!(17.32)),
                date: NaiveDate::from_ymd_opt(2025, 10, 21).expect("x"),
            },
            Txn {
                payee: "Winner".to_string(),
                amount: USD::new(dec!(9.18)),
                date: NaiveDate::from_ymd_opt(2025, 10, 23).expect("x"),
            },
            Txn {
                payee: "JetBlue".to_string(),
                amount: USD::new(dec!(345.85)),
                date: NaiveDate::from_ymd_opt(2025, 10, 24).expect("x"),
            },
            Txn {
                payee: "Mountain House Manhattan".to_string(),
                amount: USD::new(dec!(55.11)),
                date: NaiveDate::from_ymd_opt(2025, 10, 24).expect("x"),
            },
            Txn {
                payee: "Corn".to_string(),
                amount: USD::new(dec!(2.80)),
                date: NaiveDate::from_ymd_opt(2025, 10, 21).expect("x"),
            },
        ],
        "testlink".to_string(),
    );
    println!("html: {}", html_string);
    panic!("x");

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
            Ok(_) => tracing::info!("Finished create-batch command successfully"),
            Err(e) => tracing::error!(e, "creating batch failed"),
        },
        cli::Commands::Reconcile {
            batch_name,
            profile,
            dry_run,
        } => match handle_reconcile(batch_name, profile, dry_run).await {
            Ok(_) => tracing::info!("Finished reconcile command successfully"),
            Err(e) => tracing::error!(e, "reconciling batch failed"),
        },
        cli::Commands::ReconcileAll { profile, dry_run } => {
            match handle_reconcile_all(profile, dry_run).await {
                Ok(_) => tracing::info!("Finished reconcile-all command successfully"),
                Err(e) => tracing::error!(e, "reconcile-all failed"),
            }
        }
    }

    if config::is_dry_run() {
        tracing::info!("Dry run ended");
    }

    drop(log_guard);
}

async fn handle_create_batch(
    start: StartArgs,
    end_date: Option<NaiveDate>,
    profile: String,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    config::set_dry_run(dry_run);
    let config = config::read_config(&profile)?;
    let start_date = cli::start_date_from_args(start);
    let end_date = end_date.or_naive_date_now();
    commands::create_batch::run(start_date, end_date, &config, &profile).await?;
    return Ok(());
}

async fn handle_reconcile(
    batch_name: String,
    profile: String,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    config::set_dry_run(dry_run);
    let config = config::read_config(&profile)?;
    commands::reconcile::reconcile_batch_name(&batch_name, &config, &profile).await?;
    return Ok(());
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
