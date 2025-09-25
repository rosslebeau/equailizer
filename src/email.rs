use crate::{config::*, usd::USD};
use jmap_client::email::EmailBodyPart;

pub async fn send_email(
    batch_label: &String,
    amount: &USD,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = jmap_client::client::Client::new()
        .credentials(config.jmap.api_key.clone())
        .connect(&config.jmap.api_session_endpoint)
        .await?;

    let mut email_req = client.build();
    let email_send_req = email_req.set_email();

    let email = email_send_req.create();
    email.from([config.jmap.sending_address.clone()]);
    email.to([config.creditor.email_address.clone()]);
    email.subject("Quail alert! Batch ready from equailizer");
    email.mailbox_ids([&config.jmap.sent_mailbox]);

    let text_body_id = EmailBodyPart::new().part_id("t1");
    email.body_value(
        "t1".to_string(),
        format!(
            "New batch ready: {}\n\nClick here to initiate Venmo request: {}",
            batch_label,
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
    <p>New batch ready: {}</p>
    <a href=\"{}\" style=\"display:inline-block; padding:10px 20px; font-size:14px; color:#ffffff;
        background-color:#007BFF; text-decoration:none; border-radius:6px;\">Request Batch</a>
</body>
</html>
",
            batch_label,
            venmo_request_link(&config.debtor.venmo_username, batch_label, amount)
        ),
    );
    email.html_body(html_body_id);

    email_req.send().await?;

    Ok(())
}

fn venmo_request_link(venmo_username: &String, batch_label: &String, amount: &USD) -> String {
    format!(
        "https://venmo.com/{}?txn=charge&note={}&amount={}",
        venmo_username, batch_label, amount
    )
}
