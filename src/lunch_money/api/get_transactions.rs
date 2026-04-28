use super::LunchMoneyClient;
use super::http::body_snippet;
use crate::error::{Error, Result};
use crate::lunch_money::model::transaction::{Transaction, TransactionId};

use chrono::NaiveDate;
use reqwest::StatusCode;
use serde::{de::DeserializeOwned, Deserialize};

const PAGE_LIMIT: u32 = 1000;

#[derive(Debug, Deserialize)]
struct TransactionsResponse {
    transactions: Vec<Transaction>,
    has_more: bool,
}

fn decode_or_api_error<T: DeserializeOwned>(
    status: StatusCode,
    body: &str,
    context: &str,
) -> Result<T> {
    if !status.is_success() {
        return Err(Error::Api(format!(
            "{context}: HTTP {status}: {}",
            body_snippet(body)
        )));
    }

    serde_json::from_str::<T>(body).map_err(|e| {
        Error::Api(format!(
            "{context}: failed to decode HTTP {status} response ({e}): {}",
            body_snippet(body)
        ))
    })
}

pub(super) async fn get_single(client: &LunchMoneyClient, id: TransactionId) -> Result<Transaction> {
    let url = format!("https://dev.lunchmoney.app/v1/transactions/{}", id);
    let request = client
        .http
        .client()
        .get(url)
        .header("Authorization", format!("Bearer {}", client.auth_token));

    let raw = client.http.send(request).await?;
    decode_or_api_error::<Transaction>(raw.status, &raw.body, &format!("get transaction {id}"))
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

pub(super) async fn get_by_date_range(
    client: &LunchMoneyClient,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<Vec<Transaction>> {
    tracing::debug!(
        start_date = %start_date.format("%Y-%m-%d"),
        end_date = %end_date.format("%Y-%m-%d"),
        "Fetching transactions by date range"
    );

    let auth_header = format!("Bearer {}", client.auth_token);
    let mut all_transactions: Vec<Transaction> = Vec::new();
    let mut offset: u32 = 0;

    loop {
        let request = client
            .http
            .client()
            .get("https://dev.lunchmoney.app/v1/transactions")
            .query(&[
                ("start_date", &start_date.to_string()),
                ("end_date", &end_date.to_string()),
                ("limit", &PAGE_LIMIT.to_string()),
                ("offset", &offset.to_string()),
            ])
            .header("Authorization", &auth_header);

        let raw = client.http.send(request).await?;
        let response: TransactionsResponse = decode_or_api_error(
            raw.status,
            &raw.body,
            &format!(
                "get transactions {} to {} (offset {offset})",
                start_date.format("%Y-%m-%d"),
                end_date.format("%Y-%m-%d")
            ),
        )?;

        let page_count = response.transactions.len() as u32;
        let has_more = response.has_more;
        all_transactions.extend(response.transactions);

        if !has_more {
            break;
        }

        offset += page_count;
        tracing::debug!(offset, "Fetching next page of transactions");
    }

    tracing::info!(
        count = all_transactions.len(),
        "Finished fetching all transactions"
    );

    Ok(all_transactions)
}
