use chrono::NaiveDate;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "equailizer")]
#[command(about = "A tool for splitting and reconciling transactions")]
pub struct Equailizer {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    CreateBatch {
        #[arg(
            required = true,
            long = "start-date",
            short = 's',
            value_name = "yyyy-mm-dd"
        )]
        start_date: NaiveDate,
        #[arg(
            required = false,
            long = "end-date",
            short = 'e',
            value_name = "yyyy-mm-dd"
        )]
        end_date: Option<NaiveDate>,
    },
    Reconcile {
        #[arg(
            required = true,
            long = "batch-name",
            short = 'b',
            value_name = "batch name"
        )]
        batch_name: String,
        #[arg(
            required = true,
            long = "start-date",
            short = 's',
            value_name = "yyyy-mm-dd"
        )]
        start_date: NaiveDate,
        #[arg(
            required = false,
            long = "end-date",
            short = 'e',
            value_name = "yyyy-mm-dd"
        )]
        end_date: Option<NaiveDate>,
    },
    ReconcileAll {},
    TestEmail {},
}
