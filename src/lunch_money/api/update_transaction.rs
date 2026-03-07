use super::LunchMoneyClient;
use crate::lunch_money::model::transaction::*;
use crate::usd::USD;
use chrono::NaiveDate;
use display_json::DebugAsJson;
use serde::{Deserialize, Serialize};

use anyhow::{Result, anyhow, bail};

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

pub(super) async fn perform_update(
    client: &LunchMoneyClient,
    txn_update: TransactionUpdate,
) -> Result<()> {
    execute(client, txn_update.0, Action::Update(txn_update.1)).await?;
    Ok(())
}

pub(super) async fn perform_split(
    client: &LunchMoneyClient,
    update: SplitUpdate,
) -> Result<SplitResponse> {
    execute(client, update.0, Action::Split(update.1))
        .await?
        .ok_or_else(|| anyhow!("no split ids found in transaction update that contained splits"))
}

pub(super) async fn perform_update_and_split(
    client: &LunchMoneyClient,
    update: TransactionAndSplitUpdate,
) -> Result<SplitResponse> {
    execute(client, update.0, Action::UpdateAndSplit(update.1, update.2))
        .await?
        .ok_or_else(|| anyhow!("no split ids found in transaction update that contained splits"))
}

// Returns Some(SplitResponse) if a split was performed, otherwise None.
async fn execute(
    client: &LunchMoneyClient,
    txn_id: TransactionId,
    action: Action,
) -> Result<Option<SplitResponse>> {
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

    if client.dry_run {
        return Ok(Some(SplitResponse {
            split_ids: vec![0, 1],
        }));
    }

    let http = reqwest::Client::new();
    let auth_header = format!("Bearer {}", client.auth_token);
    let url = format!("https://dev.lunchmoney.app/v1/transactions/{}", txn_id);
    let response = http
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
                Ok(s.split.map(|x| SplitResponse { split_ids: x }))
            } else {
                bail!("transaction not updated, no error given")
            }
        }
        Response::Error(e) => {
            bail!(
                "{}",
                e.error
                    .first()
                    .cloned()
                    .unwrap_or_else(|| format!("unspecified error with response code {}", http_code))
            )
        }
    }
}
