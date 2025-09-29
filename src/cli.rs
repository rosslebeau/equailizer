use crate::date_helpers;
use chrono::NaiveDate;
use clap::{ArgAction, Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "equailizer")]
#[command(about = "A tool for splitting and reconciling transactions")]
pub struct Equailizer {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Args, Debug)]
#[group(required = true, multiple = false)]
pub struct StartArgs {
    #[arg(long = "start-date", short = 's', value_name = "yyyy-mm-dd")]
    pub start_date: Option<NaiveDate>,
    #[arg(long = "start-days-ago", short = 'a')]
    pub start_days_ago: Option<u16>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    CreateBatch {
        #[command(flatten)]
        start: StartArgs,
        #[arg(
            required = false,
            long = "end-date",
            short = 'e',
            value_name = "yyyy-mm-dd"
        )]
        end_date: Option<NaiveDate>,
        #[arg(required = true, long = "profile", short = 'p')]
        profile: String,
        #[arg(short, long, action = ArgAction::SetTrue)]
        dry_run: bool,
    },
    Reconcile {
        #[arg(
            required = true,
            long = "batch-name",
            short = 'b',
            value_name = "batch name"
        )]
        batch_name: String,
        #[command(flatten)]
        start: StartArgs,
        #[arg(
            required = false,
            long = "end-date",
            short = 'e',
            value_name = "yyyy-mm-dd"
        )]
        end_date: Option<NaiveDate>,
        #[arg(required = true, long = "profile", short = 'p')]
        profile: String,
        #[arg(short, long, action = ArgAction::SetTrue)]
        dry_run: bool,
    },
    ReconcileAll {
        #[arg(required = true, long = "profile", short = 'p')]
        profile: String,
        #[arg(short, long, action = ArgAction::SetTrue)]
        dry_run: bool,
    },
}

pub fn start_date_from_args(args: StartArgs) -> NaiveDate {
    match (args.start_date, args.start_days_ago) {
        (Some(date), None) => date,
        (None, Some(days_ago)) => {
            date_helpers::now_date_naive_eastern() - chrono::Days::new(days_ago.into())
        }
        (Some(date), Some(_)) => date,
        (None, None) => {
            unreachable!("clap lib error: either 'start date' or 'start days ago' must be provided")
        }
    }
}
