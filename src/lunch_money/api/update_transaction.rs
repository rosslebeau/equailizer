use super::Client;
use crate::lunch_money::model::transaction::*;
use crate::usd::USD;
use crate::{config, lunch_money::model::transaction::TransactionId};
use chrono::NaiveDate;
use display_json::DebugAsJson;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq)]
pub enum Action {
    Update(TransactionUpdate),
    Split(Vec<Split>),
    UpdateAndSplit(TransactionUpdate, Vec<Split>),
}

// This model is only used to perform this split action
#[derive(Debug, Serialize, PartialEq)]
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

#[derive(Debug, Serialize, PartialEq)]
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

pub struct Response {
    pub splits: Option<Vec<TransactionId>>,
}

impl Client {
    pub async fn update_txn_only(
        &self,
        txn_id: TransactionId,
        txn_update: &TransactionUpdate,
    ) -> Result<Response, Box<dyn std::error::Error>> {
        self.update(txn_id, Some(txn_update), None).await
    }

    #[allow(dead_code)]
    pub async fn update_split_only(
        &self,
        txn_id: TransactionId,
        splits: &Vec<Split>,
    ) -> Result<Response, Box<dyn std::error::Error>> {
        self.update(txn_id, None, Some(splits)).await
    }

    pub async fn update_txn_and_split(
        &self,
        txn_id: TransactionId,
        txn_update: &TransactionUpdate,
        splits: &Vec<Split>,
    ) -> Result<Response, Box<dyn std::error::Error>> {
        self.update(txn_id, Some(txn_update), Some(splits)).await
    }

    // TODO: Refactor txn_id into Action
    pub async fn update2(
        &self,
        txn_id: TransactionId,
        action: Action,
    ) -> Result<Response, Box<dyn std::error::Error>> {
        #[derive(Debug, Deserialize)]
        struct SuccessResponse {
            updated: bool,
            split: Option<Vec<TransactionId>>,
        }

        #[derive(Debug, Deserialize)]
        struct ErrorResponse {
            error: Vec<String>,
        }

        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        enum Response {
            Success(SuccessResponse),
            Error(ErrorResponse),
        }

        #[derive(DebugAsJson, Serialize)]
        struct RequestBodySource<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            transaction: Option<&'a TransactionUpdate>,

            #[serde(skip_serializing_if = "Option::is_none")]
            split: Option<&'a Vec<Split>>,
        }

        let txn_update_body = match &action {
            Action::Update(update) => RequestBodySource {
                transaction: Some(update),
                split: None,
            },
            Action::Split(splits) => RequestBodySource {
                transaction: None,
                split: Some(splits),
            },
            Action::UpdateAndSplit(update, splits) => RequestBodySource {
                transaction: Some(update),
                split: Some(splits),
            },
        };

        tracing::debug!(txn_id, ?txn_update_body, "updating transaction");

        if config::is_dry_run() {
            return Ok(self::Response {
                splits: Some(vec![0, 1]),
            });
        }

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
        let result: Response = response.json().await?;

        tracing::debug!(?result, "Received transaction update response");

        match result {
            Response::Success(s) => {
                if s.updated {
                    return Ok(self::Response { splits: s.split });
                } else {
                    return Err("http 200 but transaction not updated".into());
                }
            }
            Response::Error(e) => {
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

    async fn update(
        &self,
        txn_id: TransactionId,
        txn_update: Option<&TransactionUpdate>,
        splits: Option<&Vec<Split>>,
    ) -> Result<Response, Box<dyn std::error::Error>> {
        #[derive(Debug, Deserialize)]
        struct SuccessResponse {
            updated: bool,
            split: Option<Vec<TransactionId>>,
        }

        #[derive(Debug, Deserialize)]
        struct ErrorResponse {
            error: Vec<String>,
        }

        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        enum Response {
            Success(SuccessResponse),
            Error(ErrorResponse),
        }

        #[derive(DebugAsJson, Serialize)]
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

        tracing::debug!(txn_id, ?txn_update_body, "updating transaction");

        if config::is_dry_run() {
            return Ok(self::Response {
                splits: Some(vec![0, 1]),
            });
        }

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
        let result: Response = response.json().await?;

        tracing::debug!(?result, "Received transaction update response");

        match result {
            Response::Success(s) => {
                if s.updated {
                    return Ok(self::Response { splits: s.split });
                } else {
                    return Err("http 200 but transaction not updated".into());
                }
            }
            Response::Error(e) => {
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
