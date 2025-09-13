use super::super::model::transaction::Transaction;
use super::Client;

use chrono::NaiveDate;
use reqwest;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct TransactionsResponse {
    transactions: Vec<Transaction>,
}

pub type GetTransactionsResult = Result<Vec<Transaction>, Box<dyn std::error::Error>>;

impl Client {
    /*  This does not do pagination. The default limit for transactions is 1000,
        which is more than enough to run once a week, which is the goal here.
        If there are more than 1000 transactions from start_date to end_date, this program will not work correctly.
    */
    pub async fn get_transactions(
        &self,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> GetTransactionsResult {
        let auth_header = format!("Bearer {}", self.auth_token);

        let client = reqwest::Client::new();
        let response: TransactionsResponse = client
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

        return Ok(response.transactions);
    }
}
