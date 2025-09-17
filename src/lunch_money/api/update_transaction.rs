use super::Client;
use crate::lunch_money::model::transaction::Id as TransactionId;
use crate::lunch_money::model::transaction::*;
use crate::usd::USD;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

// This model is only used to perform this split action
#[derive(Debug, Serialize)]
pub struct Split {
    pub amount: USD,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_id: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<NaiveDate>,
}

#[derive(Debug, Serialize)]
pub struct TransactionUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_id: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<TransactionStatus>,
}

#[derive(Debug, Deserialize)]
struct UpdateTransactionSuccess {
    updated: bool,
    #[allow(dead_code)]
    split: Option<Vec<TransactionId>>,
}

#[derive(Debug, Deserialize)]
struct UpdateTransactionError {
    error: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum UpdateTransactionResponse {
    Success(UpdateTransactionSuccess),
    Error(UpdateTransactionError),
}

impl Client {
    pub async fn update_txn_only(
        &self,
        txn_id: TransactionId,
        txn_update: &TransactionUpdate,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.update(txn_id, Some(txn_update), None).await
    }

    #[allow(dead_code)]
    pub async fn update_split_only(
        &self,
        txn_id: TransactionId,
        splits: &Vec<Split>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.update(txn_id, None, Some(splits)).await
    }

    pub async fn update_txn_and_split(
        &self,
        txn_id: TransactionId,
        txn_update: &TransactionUpdate,
        splits: &Vec<Split>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.update(txn_id, Some(txn_update), Some(splits)).await
    }

    async fn update(
        &self,
        txn_id: TransactionId,
        txn_update: Option<&TransactionUpdate>,
        splits: Option<&Vec<Split>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        #[derive(Debug, Serialize)]
        struct RequestBodySource<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            transaction: Option<&'a TransactionUpdate>,

            #[serde(skip_serializing_if = "Option::is_none")]
            split: Option<&'a Vec<Split>>,
        }

        let txn_update_body = match (txn_update, splits) {
            (None, None) => return Err("txn_update and splits cannot both be None".into()),
            (txn_update, splits) => RequestBodySource {
                transaction: txn_update,
                split: splits,
            },
        };

        let client = reqwest::Client::new();
        let auth_header = format!("Bearer {}", self.auth_token);
        let url = format!("https://dev.lunchmoney.app/v1/transactions/{}", txn_id);
        let response = client
            .put(url)
            .header("Authorization", auth_header)
            .json(&txn_update_body)
            .send()
            .await?;

        let http_code = response.status();
        let result: UpdateTransactionResponse = response.json().await?;

        match result {
            UpdateTransactionResponse::Success(s) => {
                if s.updated {
                    return Ok(());
                } else {
                    return Err("http 200 but transaction not updated".into());
                }
            }
            UpdateTransactionResponse::Error(e) => {
                return Err(e
                    .error
                    .first()
                    .unwrap_or(&format!(
                        "unspecified error with response code {}",
                        http_code
                    ))
                    .to_owned()
                    .into());
            }
        }
    }
}
