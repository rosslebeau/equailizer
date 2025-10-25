use crate::{
    config::{self, *},
    usd::USD,
};
use jmap_client::{core::response::MethodResponse::*, email::EmailBodyPart};

pub async fn send_email(
    batch_label: &String,
    amount: &USD,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    tracing::debug!(
        config.jmap.sending_address,
        config.creditor.email_address,
        "sending email"
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

    let text_body_id = EmailBodyPart::new().part_id("t1");
    email.body_value(
        "t1".to_string(),
        format!(
            "New batch ready!\n\nClick here to initiate Venmo request: {}",
            venmo_request_link(&config.debtor.venmo_username, batch_label, amount)
        ),
    );
    email.text_body(text_body_id);

    let html_body_id = EmailBodyPart::new().part_id("t2");
    email.body_value(
        "t2".to_string(),
        format!(
            "
<html>
<body>
    <p>New batch ready!</p>
    <a href=\"{}\" style=\"display:inline-block; padding:10px 20px; font-size:14px; color:#ffffff;
        background-color:#007BFF; text-decoration:none; border-radius:6px;\">Request Batch</a>
</body>
</html>
",
            venmo_request_link(&config.debtor.venmo_username, batch_label, amount)
        ),
    );
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

fn venmo_request_link(venmo_username: &String, batch_label: &String, amount: &USD) -> String {
    format!(
        "https://venmo.com/{}?txn=charge&note={}&amount={}",
        venmo_username, batch_label, amount
    )
}
