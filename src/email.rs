use crate::{
    config::{self, *},
    date_helpers,
    usd::USD,
};
use askama::Template;
use chrono::NaiveDate;
use jmap_client::{core::response::MethodResponse::*, email::EmailBodyPart};
use std::collections::HashMap;

pub struct Txn {
    pub payee: String,
    pub amount: USD,
    pub date: NaiveDate,
}

pub async fn send_email(
    batch_id: &String,
    amount: &USD,
    txns: Vec<Txn>,
    warnings: Vec<String>,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    tracing::debug!(
        config.jmap.sending_address,
        config.creditor.email_address,
        "Sending email"
    );

    if config::is_dry_run() {
        return Ok(());
    }

    let client = jmap_client::client::Client::new()
        .credentials(config.jmap.api_key.clone())
        .connect(&config.jmap.api_session_endpoint)
        .await?;

    let mut identity_req = client.build();
    let identity_get_req = identity_req.get_identity();
    identity_get_req.account_id(client.default_account_id());
    identity_req.using.push(jmap_client::URI::Submission);

    let identity_response_missing_err: Box<dyn std::error::Error> =
        Box::from("get identity response missing");
    let no_matching_identity_err: Box<dyn std::error::Error> =
        Box::from("no identity matching config's sending address");

    let sending_identity = identity_req
        .send()
        .await?
        .pop_method_response()
        .ok_or(identity_response_missing_err)?
        .unwrap_get_identity()?
        .list()
        .iter()
        .filter_map(|x| {
            if x.email()? == config.jmap.sending_address
                && let Some(sending_id) = x.id()
            {
                Some(sending_id.to_string())
            } else {
                None
            }
        })
        .collect::<Vec<String>>()
        .first()
        .ok_or(no_matching_identity_err)?
        .clone();

    let mut email_req = client.build();
    let email_set_req = email_req.set_email();

    let email = email_set_req.create_with_id("m0");
    email.from([config.jmap.sending_address.clone()]);
    email.to([config.creditor.email_address.clone()]);
    email.subject("Quail alert! Batch ready from equailizer");
    email.mailbox_ids([&config.jmap.sent_mailbox]);

    let venmo_text = format!("equailizer_{}", date_helpers::now_date_naive_eastern());
    let venmo_request_link = venmo_request_link(&config.debtor.venmo_username, &venmo_text, amount);

    let text_body_id = EmailBodyPart::new().part_id("t1");
    email.body_value(
        "t1".to_string(),
        format!(
            "New batch ready!\n\nClick here to initiate Venmo request: {}\n\nbatch id: {}",
            venmo_request_link, batch_id
        ),
    );
    email.text_body(text_body_id);

    let html_body_id = EmailBodyPart::new().part_id("t2");
    let html_text = make_html_string(txns, venmo_request_link, warnings);
    email.body_value("t2".to_string(), html_text);
    email.html_body(html_body_id);

    let email_response = match email_req.send().await?.pop_method_response() {
        Some(res) => res.unwrap_method_response(),
        None => {
            return Err("JMAP create email response did not contain any methodResponses".into());
        }
    };

    let no_id_err: Box<dyn std::error::Error> =
        Box::from("didn't find email submission id in response");
    let email_id = match email_response {
        SetEmail(mut es) => es.created("m0")?.id().ok_or(no_id_err)?.to_string(),
        _ => return Err("JMAP create email response was not of type SetEmail".into()),
    };

    client
        .email_submission_create(email_id, sending_identity)
        .await?;

    tracing::debug!(
        config.jmap.sending_address,
        config.creditor.email_address,
        "email sent"
    );

    Ok(())
}

fn venmo_request_link(venmo_username: &String, text: &String, amount: &USD) -> String {
    format!(
        "https://venmo.com/{}?txn=charge&note={}&amount={}",
        venmo_username, text, amount
    )
}

#[derive(Template)] // this will generate the code...
#[template(path = "batch_ready_email.html")]
struct BatchReadyEmailTemplate {
    txns_by_date: HashMap<String, Vec<Txn>>,
    venmo_request_link: String,
    warnings: Vec<String>,
}

pub fn make_html_string(
    txns: Vec<Txn>,
    venmo_request_link: String,
    warnings: Vec<String>,
) -> String {
    let txns_by_date: HashMap<String, Vec<Txn>> =
        txns.into_iter()
            .fold(HashMap::<String, Vec<Txn>>::new(), |mut acc, txn| {
                acc.entry(txn.date.format("%b %d, %Y").to_string())
                    .or_insert_with(Vec::new)
                    .push(txn);
                acc
            });

    let email = BatchReadyEmailTemplate {
        txns_by_date: txns_by_date,
        venmo_request_link: venmo_request_link,
        warnings: warnings,
    };

    return email.render().unwrap();
}

#[cfg(debug_assertions)]
pub fn dev_print(batch_id: &String, txns: Vec<Txn>, warnings: Vec<String>) {
    use std::fs;

    let mut path = std::env::current_exe()
        .expect("no current exe??")
        .parent()
        .expect("couldn't open exe's path")
        .to_path_buf();

    if cfg!(debug_assertions) {
        path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    }

    path.push("dev");

    if !path.is_dir() {
        if path.exists() {
            panic!("cannot create or open data directory - non-directory file exists at this path");
        } else {
            fs::create_dir_all(path.as_path()).expect("failed to make dev path");
        }
    }

    let file_path = path.join("email.html");

    let html = make_html_string(txns, "https://www.example.com".to_string(), warnings);

    fs::write(file_path, html).expect("failed to write html file");
}
