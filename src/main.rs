mod cli;
mod log;

use equailizer::lunch_money::api::LunchMoneyClient;
use equailizer::lunch_money::model::transaction::TransactionId;
use equailizer::usd::USD;
use chrono::NaiveDate;
use clap::Parser;

use cli::StartArgs;
use equailizer::date_helpers::*;
use equailizer::email::Txn;

#[tokio::main]
async fn main() {
    let log_guard = log::init_tracing();

    let args = cli::Equailizer::parse();

    match args.command {
        cli::Commands::CreateBatch {
            start,
            end_date,
            profile,
            dry_run,
        } => {
            if dry_run {
                tracing::info!("dry run beginning");
            }
            match handle_create_batch(start, end_date, profile, dry_run).await {
                Ok(_) => tracing::info!("Finished create-batch command successfully"),
                Err(e) => tracing::error!("{e:#}", e = e),
            }
            if dry_run {
                tracing::info!("Dry run ended");
            }
        }
        cli::Commands::Reconcile {
            batch_name,
            profile,
            dry_run,
        } => {
            if dry_run {
                tracing::info!("dry run beginning");
            }
            match handle_reconcile(batch_name, profile, dry_run).await {
                Ok(_) => tracing::info!("Finished reconcile command successfully"),
                Err(e) => tracing::error!("{e:#}", e = e),
            }
            if dry_run {
                tracing::info!("Dry run ended");
            }
        }
        cli::Commands::ReconcileAll { profile, dry_run } => {
            if dry_run {
                tracing::info!("dry run beginning");
            }
            match handle_reconcile_all(profile, dry_run).await {
                Ok(_) => tracing::info!("Finished reconcile-all command successfully"),
                Err(e) => tracing::error!("{e:#}", e = e),
            }
            if dry_run {
                tracing::info!("Dry run ended");
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

    drop(log_guard);
}

async fn handle_create_batch(
    start: StartArgs,
    end_date: Option<NaiveDate>,
    profile: String,
    dry_run: bool,
) -> anyhow::Result<()> {
    let config = equailizer::config::read_config(&profile)?;
    let start_date = cli::start_date_from_args(start);
    let end_date = end_date.or_naive_date_now();

    let api = LunchMoneyClient {
        auth_token: config.creditor.api_key.clone(),
        dry_run,
    };
    let persistence = equailizer::persist::FilePersistence::new(&profile, dry_run)?;
    let email_sender = equailizer::email::JmapEmailSender {
        api_session_endpoint: config.jmap.api_session_endpoint.clone(),
        api_key: config.jmap.api_key.clone(),
        sent_mailbox: config.jmap.sent_mailbox.clone(),
        sending_address: config.jmap.sending_address.clone(),
        creditor_email: config.creditor.email_address.clone(),
        debtor_email: config.debtor.email_address.clone(),
        debtor_venmo_username: config.debtor.venmo_username.clone(),
        dry_run,
    };

    equailizer::commands::create_batch::create_batch(
        start_date,
        end_date,
        &config,
        &api,
        &persistence,
        &email_sender,
    )
    .await
}

async fn handle_reconcile(
    batch_name: String,
    profile: String,
    dry_run: bool,
) -> anyhow::Result<()> {
    let config = equailizer::config::read_config(&profile)?;
    let creditor_api = LunchMoneyClient {
        auth_token: config.creditor.api_key.clone(),
        dry_run,
    };
    let debtor_api = LunchMoneyClient {
        auth_token: config.debtor.api_key.clone(),
        dry_run,
    };
    let persistence = equailizer::persist::FilePersistence::new(&profile, dry_run)?;

    equailizer::commands::reconcile::reconcile_batch_name(
        &batch_name,
        &config,
        &creditor_api,
        &debtor_api,
        &persistence,
    )
    .await
}

async fn handle_reconcile_all(profile: String, dry_run: bool) -> anyhow::Result<()> {
    let config = equailizer::config::read_config(&profile)?;
    let creditor_api = LunchMoneyClient {
        auth_token: config.creditor.api_key.clone(),
        dry_run,
    };
    let debtor_api = LunchMoneyClient {
        auth_token: config.debtor.api_key.clone(),
        dry_run,
    };
    let persistence = equailizer::persist::FilePersistence::new(&profile, dry_run)?;

    equailizer::commands::reconcile::reconcile_all(
        &config,
        &creditor_api,
        &debtor_api,
        &persistence,
    )
    .await
}

fn handle_dev_email() {
    let txns: Vec<Txn> = vec![
        Txn {
            payee: "Associated Market".to_string(),
            amount: USD::new_from_cents(2531),
            date: NaiveDate::from_ymd_opt(2025, 10, 21).expect("NaiveDate creation failed"),
            notes: Some("test note".to_string()),
        },
        Txn {
            payee: "Associated Market".to_string(),
            amount: USD::new_from_cents(2531),
            date: NaiveDate::from_ymd_opt(2025, 10, 21).expect("NaiveDate creation failed"),
            notes: None,
        },
        Txn {
            payee: "Associated Market".to_string(),
            amount: USD::new_from_cents(2531),
            date: NaiveDate::from_ymd_opt(2025, 10, 21).expect("NaiveDate creation failed"),
            notes: Some("even more test notes".to_string()),
        },
        Txn {
            payee: "Associated Market".to_string(),
            amount: USD::new_from_cents(2531),
            date: NaiveDate::from_ymd_opt(2025, 10, 21).expect("NaiveDate creation failed"),
            notes: Some("testing again note".to_string()),
        },
        Txn {
            payee: "Associated Market".to_string(),
            amount: USD::new_from_cents(2531),
            date: NaiveDate::from_ymd_opt(2025, 10, 21).expect("NaiveDate creation failed"),
            notes: None,
        },
    ];

    let warnings = vec!["Test warning: could not find something".to_string()];

    let total = USD::new_from_cents(10842);
    equailizer::email::dev_print(&uuid::Uuid::new_v4().to_string(), txns, warnings, &total);
}

async fn handle_dev_txn(id: TransactionId, profile: String) {
    use equailizer::lunch_money::api::LunchMoney;
    let config = equailizer::config::read_config(&profile).expect("failed reading config");
    let client = LunchMoneyClient {
        auth_token: config.creditor.api_key.to_owned(),
        dry_run: false,
    };
    let txn = client
        .get_transaction(id)
        .await
        .expect("failed getting txn");
    tracing::info!("Got transaction: {:?}", txn);
}
