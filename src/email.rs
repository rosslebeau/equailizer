use crate::date_helpers;
use crate::usd::USD;
use anyhow::Result;
use askama::Template;
use async_trait::async_trait;
use chrono::NaiveDate;
use jmap_client::{client::Client, core::response::MethodResponse::*, email::EmailBodyPart};
use std::collections::BTreeMap;

pub struct Txn {
    pub payee: String,
    pub amount: USD,
    pub date: NaiveDate,
    #[allow(unused_variables)]
    // This is used in the askama html template, which isn't seen by the linter
    pub notes: Option<String>,
}

#[async_trait]
pub trait EmailSender: Send + Sync {
    async fn send_batch_emails(
        &self,
        batch_id: &str,
        total: &USD,
        txns: &[Txn],
        warnings: Vec<String>,
    ) -> Result<()>;
}

pub struct JmapEmailSender {
    pub api_session_endpoint: String,
    pub api_key: String,
    pub sent_mailbox: String,
    pub sending_address: String,
    pub creditor_email: String,
    pub debtor_email: String,
    pub debtor_venmo_username: String,
    pub dry_run: bool,
}

#[async_trait]
impl EmailSender for JmapEmailSender {
    async fn send_batch_emails(
        &self,
        batch_id: &str,
        total: &USD,
        txns: &[Txn],
        warnings: Vec<String>,
    ) -> Result<()> {
        let client = jmap_client::client::Client::new()
            .credentials(self.api_key.clone())
            .connect(&self.api_session_endpoint)
            .await?;

        let mut identity_req = client.build();
        let identity_get_req = identity_req.get_identity();
        identity_get_req.account_id(client.default_account_id());
        identity_req.using.push(jmap_client::URI::Submission);

        let sending_identity = identity_req
            .send()
            .await?
            .pop_method_response()
            .ok_or_else(|| anyhow::anyhow!("get identity response missing"))?
            .unwrap_get_identity()?
            .list()
            .iter()
            .filter_map(|x| {
                if x.email()? == self.sending_address
                    && let Some(sending_id) = x.id()
                {
                    Some(sending_id.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<String>>()
            .first()
            .ok_or_else(|| anyhow::anyhow!("no identity matching config's sending address"))?
            .clone();

        self.send_creditor_email(
            &client,
            &sending_identity,
            batch_id,
            total,
            txns,
            warnings,
        )
        .await?;

        self.send_debtor_email(&client, &sending_identity, batch_id, total, txns)
            .await?;

        Ok(())
    }
}

impl JmapEmailSender {
    async fn send_creditor_email(
        &self,
        client: &Client,
        sending_identity: &str,
        batch_id: &str,
        total: &USD,
        txns: &[Txn],
        warnings: Vec<String>,
    ) -> Result<()> {
        tracing::debug!(
            self.sending_address,
            self.creditor_email,
            "Sending creditor email"
        );

        if self.dry_run {
            return Ok(());
        }

        let mut email_req = client.build();
        let email_set_req = email_req.set_email();

        let email = email_set_req.create_with_id("m0");
        email.from([self.sending_address.clone()]);
        email.to([self.creditor_email.clone()]);
        email.subject("Quail alert! Batch ready from equailizer");
        email.mailbox_ids([&self.sent_mailbox]);

        let venmo_text = format!("equailizer_{}", date_helpers::now_date_naive_eastern());
        let venmo_request_link =
            venmo_request_link(&self.debtor_venmo_username, &venmo_text, total);

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
        let html_text = make_creditor_email_html_string(
            txns,
            &venmo_request_link,
            warnings,
            &batch_id.to_string(),
            total,
        );
        email.body_value("t2".to_string(), html_text);
        email.html_body(html_body_id);

        let email_response = match email_req.send().await?.pop_method_response() {
            Some(res) => res.unwrap_method_response(),
            None => {
                return Err(
                    anyhow::anyhow!("JMAP create email response did not contain any methodResponses")
                );
            }
        };

        let email_id = match email_response {
            SetEmail(mut es) => es
                .created("m0")?
                .id()
                .ok_or_else(|| anyhow::anyhow!("didn't find email submission id in response"))?
                .to_string(),
            _ => return Err(anyhow::anyhow!("JMAP create email response was not of type SetEmail")),
        };

        client
            .email_submission_create(email_id, sending_identity.to_string())
            .await?;

        tracing::debug!(
            self.sending_address,
            self.creditor_email,
            "creditor email sent"
        );

        Ok(())
    }

    async fn send_debtor_email(
        &self,
        client: &Client,
        sending_identity: &str,
        batch_id: &str,
        total: &USD,
        txns: &[Txn],
    ) -> Result<()> {
        if self.dry_run {
            return Ok(());
        }

        let mut email_req = client.build();
        let email_set_req = email_req.set_email();

        let email = email_set_req.create_with_id("m0");
        email.from([self.sending_address.clone()]);
        email.to([self.debtor_email.clone()]);
        email.subject("Quail alert! Batch incoming from equailizer");
        email.mailbox_ids([&self.sent_mailbox]);

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
        let html_text =
            make_debtor_email_html_string(txns, &batch_id.to_string(), total);
        email.body_value("t2".to_string(), html_text);
        email.html_body(html_body_id);

        let email_response = match email_req.send().await?.pop_method_response() {
            Some(res) => res.unwrap_method_response(),
            None => {
                return Err(
                    anyhow::anyhow!("JMAP create email response did not contain any methodResponses")
                );
            }
        };

        let email_id = match email_response {
            SetEmail(mut es) => es
                .created("m0")?
                .id()
                .ok_or_else(|| anyhow::anyhow!("didn't find email submission id in response"))?
                .to_string(),
            _ => return Err(anyhow::anyhow!("JMAP create email response was not of type SetEmail")),
        };

        client
            .email_submission_create(email_id, sending_identity.to_string())
            .await?;

        tracing::debug!(
            self.sending_address,
            self.debtor_email,
            "debtor email sent"
        );

        Ok(())
    }
}

fn venmo_request_link(venmo_username: &str, text: &str, amount: &USD) -> String {
    format!(
        "https://venmo.com/{}?txn=charge&note={}&amount={}",
        venmo_username, text, amount
    )
}

#[derive(Template)]
#[template(path = "batch_ready_creditor_email.html")]
struct BatchReadyEmailTemplate<'a> {
    txns_by_date: BTreeMap<NaiveDate, Vec<&'a Txn>>,
    venmo_request_link: &'a String,
    warnings: Vec<String>,
    batch_id: &'a String,
    total: &'a USD,
}

pub fn make_creditor_email_html_string(
    txns: &[Txn],
    venmo_request_link: &String,
    warnings: Vec<String>,
    batch_id: &String,
    total: &USD,
) -> String {
    let txns_by_date = group_txns_by_date(txns);

    let email = BatchReadyEmailTemplate {
        txns_by_date,
        venmo_request_link,
        warnings,
        batch_id,
        total,
    };

    email.render().unwrap()
}

#[derive(Template)]
#[template(path = "batch_ready_debtor_email.html")]
struct BatchReadyDebtorEmailTemplate<'a> {
    txns_by_date: BTreeMap<NaiveDate, Vec<&'a Txn>>,
    batch_id: &'a String,
    total: &'a USD,
}

pub fn make_debtor_email_html_string(txns: &[Txn], batch_id: &String, total: &USD) -> String {
    let txns_by_date = group_txns_by_date(txns);

    let email = BatchReadyDebtorEmailTemplate {
        txns_by_date,
        batch_id,
        total,
    };

    email.render().unwrap()
}

fn group_txns_by_date<'a>(txns: &'a [Txn]) -> BTreeMap<NaiveDate, Vec<&'a Txn>> {
    txns.iter()
        .fold(BTreeMap::<NaiveDate, Vec<&Txn>>::new(), |mut acc, txn| {
            acc.entry(txn.date).or_insert_with(Vec::new).push(txn);
            acc
        })
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
