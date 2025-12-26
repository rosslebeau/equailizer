use crate::{
    config::{self, *},
    date_helpers,
    usd::USD,
};
use askama::Template;
use chrono::NaiveDate;
use jmap_client::{client::Client, core::response::MethodResponse::*, email::EmailBodyPart};
use std::collections::HashMap;

pub struct Txn {
    pub payee: String,
    pub amount: USD,
    pub date: NaiveDate,
    #[allow(unused_variables)]
    // This is used in the askama html template, which isn't seen by the linter
    pub notes: Option<String>,
}

fn venmo_request_link(venmo_username: &String, text: &String, amount: &USD) -> String {
    format!(
        "https://venmo.com/{}?txn=charge&note={}&amount={}",
        venmo_username, text, amount
    )
}

pub async fn send_emails(
    batch_id: &String,
    total: &USD,
    txns: Vec<Txn>,
    warnings: Vec<String>,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
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

    send_creditor_email(
        &client,
        config.jmap.sending_address.clone(),
        sending_identity.clone(),
        config.jmap.sent_mailbox.clone(),
        config.creditor.email_address.clone(),
        config.debtor.venmo_username.clone(),
        batch_id,
        total,
        &txns,
        warnings,
    )
    .await?;

    send_debtor_email(
        &client,
        config.jmap.sending_address.clone(),
        sending_identity.clone(),
        config.jmap.sent_mailbox.clone(),
        config.debtor.email_address.clone(),
        batch_id,
        total,
        &txns,
    )
    .await?;

    Ok(())
}

async fn send_creditor_email(
    client: &Client,
    sending_address: String,
    sending_identity: String,
    sent_mailbox: String,
    creditor_address: String,
    debtor_venmo_username: String,
    batch_id: &String,
    total: &USD,
    txns: &Vec<Txn>,
    warnings: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    tracing::debug!(sending_address, creditor_address, "Sending creditor email");

    if config::is_dry_run() {
        return Ok(());
    }

    let mut email_req = client.build();
    let email_set_req = email_req.set_email();

    let email = email_set_req.create_with_id("m0");
    email.from([sending_address.clone()]);
    email.to([creditor_address.clone()]);
    email.subject("Quail alert! Batch ready from equailizer");
    email.mailbox_ids([&sent_mailbox]);

    let venmo_text = format!("equailizer_{}", date_helpers::now_date_naive_eastern());
    let venmo_request_link = venmo_request_link(&debtor_venmo_username, &venmo_text, total);

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
    let html_text =
        make_creditor_email_html_string(txns, &venmo_request_link, warnings, batch_id, total);
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

    tracing::debug!(sending_address, creditor_address, "creditor email sent");

    Ok(())
}

async fn send_debtor_email(
    client: &Client,
    sending_address: String,
    sending_identity: String,
    sent_mailbox: String,
    debtor_address: String,
    batch_id: &String,
    total: &USD,
    txns: &Vec<Txn>,
) -> Result<(), Box<dyn std::error::Error>> {
    // tracing::debug!(
    //     config.jmap.sending_address,
    //     config.creditor.email_address,
    //     "Sending debtor email"
    // );

    if config::is_dry_run() {
        return Ok(());
    }

    let mut email_req = client.build();
    let email_set_req = email_req.set_email();

    let email = email_set_req.create_with_id("m0");
    email.from([sending_address.clone()]);
    email.to([debtor_address.clone()]);
    email.subject("Quail alert! Batch incoming from equailizer");
    email.mailbox_ids([&sent_mailbox]);

    let text_body_id = EmailBodyPart::new().part_id("t1");
    email.body_value(
        "t1".to_string(),
        format!(
            "New batch incoming! You'll see a venmo request for it soon.\n\nbatch id: {}",
            batch_id
        ),
    );
    email.text_body(text_body_id);

    let html_body_id = EmailBodyPart::new().part_id("t2");
    let html_text = make_debtor_email_html_string(txns, batch_id, total);
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

    tracing::debug!(sending_address, debtor_address, "debtor email sent");

    Ok(())
}

#[derive(Template)]
#[template(path = "batch_ready_creditor_email.html")]
struct BatchReadyEmailTemplate<'a> {
    txns_by_date: HashMap<String, Vec<&'a Txn>>,
    venmo_request_link: &'a String,
    warnings: Vec<String>,
    batch_id: &'a String,
    total: &'a USD,
}

pub fn make_creditor_email_html_string(
    txns: &Vec<Txn>,
    venmo_request_link: &String,
    warnings: Vec<String>,
    batch_id: &String,
    total: &USD,
) -> String {
    let txns_by_date: HashMap<String, Vec<&Txn>> =
        txns.into_iter()
            .fold(HashMap::<String, Vec<&Txn>>::new(), |mut acc, txn| {
                acc.entry(txn.date.format("%b %d, %Y").to_string())
                    .or_insert_with(Vec::new)
                    .push(txn);
                acc
            });

    let email = BatchReadyEmailTemplate {
        txns_by_date: txns_by_date,
        venmo_request_link: venmo_request_link,
        warnings: warnings,
        batch_id: batch_id,
        total: total,
    };

    return email.render().unwrap();
}

#[derive(Template)]
#[template(path = "batch_ready_debtor_email.html")]
struct BatchReadyDebtorEmailTemplate<'a> {
    txns_by_date: HashMap<String, Vec<&'a Txn>>,
    batch_id: &'a String,
    total: &'a USD,
}

pub fn make_debtor_email_html_string(txns: &Vec<Txn>, batch_id: &String, total: &USD) -> String {
    let txns_by_date: HashMap<String, Vec<&Txn>> =
        txns.into_iter()
            .fold(HashMap::<String, Vec<&Txn>>::new(), |mut acc, txn| {
                acc.entry(txn.date.format("%b %d, %Y").to_string())
                    .or_insert_with(Vec::new)
                    .push(txn);
                acc
            });

    let email = BatchReadyDebtorEmailTemplate {
        txns_by_date: txns_by_date,
        batch_id: batch_id,
        total: total,
    };

    return email.render().unwrap();
}

#[cfg(debug_assertions)]
pub fn dev_print(batch_id: &String, txns: Vec<Txn>, warnings: Vec<String>, amount: &USD) {
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

    let creditor_file_path = path.join("email_creditor.html");

    let creditor_html = make_creditor_email_html_string(
        &txns,
        &("https://www.example.com".to_string()),
        warnings,
        batch_id,
        amount,
    );

    fs::write(creditor_file_path, creditor_html).expect("failed to write html file");

    let debtor_html = make_debtor_email_html_string(&txns, batch_id, amount);
    let debtor_file_path = path.join("email_debtor.html");
    fs::write(debtor_file_path, debtor_html).expect("failed to write html file");
}
