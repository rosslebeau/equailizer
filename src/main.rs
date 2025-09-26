mod cli;
mod commands;
mod config;
mod date_helpers;
mod email;
mod lunch_money;
mod persist;
pub mod usd;

use clap::Parser;

use crate::usd::USD;
use date_helpers::*;
use rust_decimal::*;

#[tokio::main]
async fn main() {
    let args = cli::Equailizer::parse();

    let read_config = config::read_config();
    let config = match read_config {
        Ok(x) => x,
        Err(e) => {
            println!("Fatal error when reading config: {e}");
            return;
        }
    };

    match args.command {
        cli::Commands::CreateBatch {
            start_date,
            end_date,
        } => {
            let end_date = end_date.or_naive_date_now();
            let result = commands::create_batch::run(start_date, end_date, &config).await;
            match result {
                Ok(res) => println!(
                    "Successfully created batch!\nBatch label: {}\nBatch amount: {}",
                    res.batch_label, res.batch_amount
                ),
                Err(e) => println!("Creating batch failed with error: {}", e),
            }
        }
        cli::Commands::Reconcile {
            batch_name,
            start_date,
            end_date,
        } => {
            let end_date = end_date.or_naive_date_now();
            let result =
                commands::reconcile::reconcile_batch(&batch_name, start_date, end_date, &config)
                    .await;
            match result {
                Ok(()) => println!("Successfully reconciled batch: {}", batch_name),
                Err(e) => println!("Creating batch failed with error: {}", e),
            }
        }
        cli::Commands::ReconcileAll {} => {
            let result = commands::reconcile::reconcile_all(&config).await;
            match result {
                Ok(batch_names) => println!(
                    "Successfully reconciled all outstanding batches:\n{}",
                    batch_names.join("\n")
                ),
                Err(e) => println!("Reconciling all batches failed with error: {}", e),
            }
        }
        cli::Commands::TestEmail {} => {
            email::send_email(&"456".to_string(), &USD::new(dec!(50.21)), &config)
                .await
                .expect("error email ouch");
        }
    }
}
