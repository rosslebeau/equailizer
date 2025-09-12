mod cli;
mod commands;
mod config;
mod lunch_money;

use clap::Parser;

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
            let result = commands::create_batch::run(&config, start_date, end_date).await;
            match result {
                Ok(res) => println!(
                    "Successfully created batch!\nBatch label: {}\nBatch amount: {}",
                    res.batch_label, res.batch_amount
                ),
                Err(e) => println!("Creating batch failed with error: {}", e),
            }
        }
    }
}
