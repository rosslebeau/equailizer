use super::LunchMoneyClient;
use crate::lunch_money::model::transaction::{Transaction, TransactionId};

use anyhow::Result;
use chrono::NaiveDate;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct TransactionsResponse {
    transactions: Vec<Transaction>,
}

pub(super) async fn get_single(client: &LunchMoneyClient, id: TransactionId) -> Result<Transaction> {
    let auth_header = format!("Bearer {}", client.auth_token);

    let url = format!("https://dev.lunchmoney.app/v1/transactions/{}", id);
    let http = reqwest::Client::new();
    let response: Transaction = http
        .get(url)
        .header("Authorization", auth_header)
        .send()
        .await?
        .json()
        .await?;

    Ok(response)
}

// The Lunch Money API does not currently have a way to request multiple transactions by id in a single call.
// Still, making multiple calls to get specific transactions can be more efficient and better logic than
// requesting a whole date range and filtering.
pub(super) async fn get_by_ids(
    client: &LunchMoneyClient,
    ids: &[TransactionId],
) -> Result<Vec<Transaction>> {
    tracing::debug!("Getting transactions with ids: {:?}", ids);
    let mut txns: Vec<Transaction> = Vec::new();
    for txn_id in ids {
        txns.push(get_single(client, *txn_id).await?);
    }
    Ok(txns)
}

/*  This does not do pagination. The default limit for transactions is 1000,
    which is more than enough to run once a week, which is the goal here.
    If there are more than 1000 transactions from start_date to end_date, this program will not work correctly.
*/
pub(super) async fn get_by_date_range(
    client: &LunchMoneyClient,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<Vec<Transaction>> {
    tracing::debug!(
        "Getting transactions by date: start date: {}, end date: {}",
        start_date.format("%m/%d/%Y"),
        end_date.format("%m/%d/%Y")
    );

    let auth_header = format!("Bearer {}", client.auth_token);

    let http = reqwest::Client::new();
    let response: TransactionsResponse = http
        .get("https://dev.lunchmoney.app/v1/transactions")
        .query(&[
            ("start_date", &start_date.to_string()),
            ("end_date", &end_date.to_string()),
        ])
        .header("Authorization", auth_header)
        .send()
        .await?
        .json()
        .await?;

    Ok(response.transactions)
}
