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

use crate::{
    commands::create_batch,
    config::Config,
    lunch_money::{
        api::Client,
        model::transaction::{Transaction, TransactionId, TransactionStatus},
    },
    usd::USD,
};
use chrono::NaiveDate;
use clap::Parser;
use rust_decimal::dec;
use uuid::Uuid;

use crate::{cli::StartArgs, email::Txn};
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
        #[cfg(debug_assertions)]
        cli::Commands::Dev(subcommand) => match subcommand {
            cli::DevSubcommand::Email {} => {
                tracing::info!("email command");
                handle_dev_email();
            }
            cli::DevSubcommand::Txn { id, profile } => {
                tracing::info!("dev txn command");
                handle_dev_txn(id, profile).await;
            }
        },
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
    commands::create_batch::create_batch(start_date, end_date, &profile, &config).await?;
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

fn handle_dev_email() {
    let txns: Vec<Txn> = vec![
        Txn {
            payee: "Associated Market".to_string(),
            amount: USD::new_from_cents(2531),
            date: NaiveDate::from_ymd_opt(2025, 10, 21).expect("NaiveDate creation failed"),
        },
        Txn {
            payee: "Associated Market".to_string(),
            amount: USD::new_from_cents(2531),
            date: NaiveDate::from_ymd_opt(2025, 10, 21).expect("NaiveDate creation failed"),
        },
        Txn {
            payee: "Associated Market".to_string(),
            amount: USD::new_from_cents(2531),
            date: NaiveDate::from_ymd_opt(2025, 10, 21).expect("NaiveDate creation failed"),
        },
        Txn {
            payee: "Associated Market".to_string(),
            amount: USD::new_from_cents(2531),
            date: NaiveDate::from_ymd_opt(2025, 10, 21).expect("NaiveDate creation failed"),
        },
        Txn {
            payee: "Associated Market".to_string(),
            amount: USD::new_from_cents(2531),
            date: NaiveDate::from_ymd_opt(2025, 10, 21).expect("NaiveDate creation failed"),
        },
    ];

    let warnings = vec!["Test warning: could not find something".to_string()];

    email::dev_print(&Uuid::new_v4().to_string(), txns, warnings);
}

async fn handle_dev_txn(id: TransactionId, profile: String) {
    let config = config::read_config(&profile).expect("failed reading config");
    let client = Client {
        auth_token: config.creditor.api_key.to_owned(),
    };
    let txn = client
        .get_transaction(id)
        .await
        .expect("failed getting txn");
    tracing::info!("Got transaction: {:?}", txn);
}
