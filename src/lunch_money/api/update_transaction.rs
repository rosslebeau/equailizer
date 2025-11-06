use super::Client;
use crate::lunch_money::model::transaction::*;
use crate::usd::USD;
use crate::{config, lunch_money::model::transaction::TransactionId};
use chrono::NaiveDate;
use display_json::DebugAsJson;
use serde::{Deserialize, Serialize};

pub type TransactionUpdate = (TransactionId, TransactionUpdateItem);

#[derive(Debug, Serialize, PartialEq)]
pub struct TransactionUpdateItem {
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

pub type SplitUpdate = (TransactionId, Vec<SplitUpdateItem>);

#[derive(Debug, Serialize, PartialEq)]
pub struct SplitUpdateItem {
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

pub type TransactionAndSplitUpdate = (TransactionId, TransactionUpdateItem, Vec<SplitUpdateItem>);

#[derive(Debug)]
enum Action {
    Update(TransactionUpdateItem),
    Split(Vec<SplitUpdateItem>),
    UpdateAndSplit(TransactionUpdateItem, Vec<SplitUpdateItem>),
}

pub struct SplitResponse {
    pub split_ids: Vec<TransactionId>,
}

impl Client {
    pub async fn update_transaction(
        &self,
        txn_update: TransactionUpdate,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.update(txn_update.0, Action::Update(txn_update.1))
            .await?;
        Ok(())
    }

    pub async fn update_split(
        &self,
        update: SplitUpdate,
    ) -> Result<SplitResponse, Box<dyn std::error::Error>> {
        self.update(update.0, Action::Split(update.1))
            .await?
            .ok_or("no split ids found in transaction update that contained splits".into())
    }

    pub async fn update_transaction_and_split(
        &self,
        update: TransactionAndSplitUpdate,
    ) -> Result<SplitResponse, Box<dyn std::error::Error>> {
        self.update(update.0, Action::UpdateAndSplit(update.1, update.2))
            .await?
            .ok_or("no split ids found in transaction update that contained splits".into())
    }

    // TODO: Refactor txn_id into Action
    // Returns Some(SplitResponse) if a split was performed, otherwise None.
    // This is handled in a type-safe way by the public methods that call into this.
    async fn update(
        &self,
        txn_id: TransactionId,
        action: Action,
    ) -> Result<Option<SplitResponse>, Box<dyn std::error::Error>> {
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
            transaction: Option<&'a TransactionUpdateItem>,

            #[serde(skip_serializing_if = "Option::is_none")]
            split: Option<&'a Vec<SplitUpdateItem>>,
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

        tracing::debug!(txn_id, ?txn_update_body, "Updating transaction");

        if config::is_dry_run() {
            return Ok(Some(SplitResponse {
                split_ids: vec![0, 1],
            }));
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
        let response: Response = response.json().await?;

        tracing::debug!(
            ?http_code,
            ?response,
            "Received transaction update response"
        );

        match response {
            Response::Success(s) => {
                if s.updated {
                    return Ok(s.split.map(|x| SplitResponse { split_ids: x }));
                } else {
                    return Err(format!("transaction not updated, no error given").into());
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
