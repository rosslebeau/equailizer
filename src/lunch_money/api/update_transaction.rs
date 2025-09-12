use super::Client;
use crate::lunch_money::model::transaction::Id as TransactionId;
use crate::lunch_money::model::transaction::{self, *};
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
    // Not adding this now as I don't need it yet, but keeping it in the file so I know it's there if I need it later.
    // date: Option<NaiveDate>,
}

#[derive(Debug, Serialize)]
pub struct TransactionUpdate {
    pub transaction_id: transaction::Id,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub payee: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<TransactionStatus>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTransactionSuccess {
    pub updated: bool,
    pub split: Option<Vec<TransactionId>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTransactionError {
    pub error: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum UpdateTransactionResponse {
    Success(UpdateTransactionSuccess),
    Error(UpdateTransactionError),
}

impl Client {
    pub async fn update_txn_only(
        &self,
        txn_update: TransactionUpdate,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.update(None, Some(txn_update), None).await
    }

    pub async fn update_split_only(
        &self,
        txn_id: TransactionId,
        splits: Vec<Split>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.update(Some(txn_id), None, Some(splits)).await
    }

    pub async fn update_txn_and_split(
        &self,
        txn_update: TransactionUpdate,
        splits: Vec<Split>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.update(None, Some(txn_update), Some(splits)).await
    }

    async fn update(
        &self,
        txn_id: Option<TransactionId>,
        txn_update: Option<TransactionUpdate>,
        splits: Option<Vec<Split>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        #[derive(Debug, Serialize)]
        struct UpdateTransactionRequestBody {
            #[serde(skip_serializing_if = "Option::is_none")]
            transaction: Option<TransactionUpdate>,

            #[serde(skip_serializing_if = "Option::is_none")]
            split: Option<Vec<Split>>,
        }

        let txn_id = match (txn_id, &txn_update) {
            (None, None) => {
                return Err(
                    "cannot update transaction without either id or transaction update item".into(),
                );
            }
            (Some(_), Some(_)) => {
                return Err("should not ever get both a txn_id and a txn_update".into());
            }
            (Some(id), None) => id,
            (None, Some(update)) => update.transaction_id,
        };

        let txn_update_body = match (txn_update, splits) {
            (None, None) => return Err("txn_update and splits cannot both be None".into()),
            (a, b) => UpdateTransactionRequestBody {
                transaction: a,
                split: b,
            },
        };

        let auth_header = format!("Bearer {}", self.auth_token);

        // TEST CODE

        // let client = reqwest::Client::new();
        // let url = format!("https://dev.lunchmoney.app/v1/transactions/:{}", txn_id);
        // let req_b = client
        //     .put(url)
        //     .header("Authorization", auth_header)
        //     .json(&txn_update_body)
        //     .build();

        // let req = match req_b {
        //     Ok(r) => r,
        //     Err(_) => return Ok(()),
        // };

        // let json: String = serde_json::to_string_pretty(&txn_update_body).expect("JSON ERR");
        // println!(
        //     "updating with\nURL: {:?}\nHeaders: {:?}\nBody: {:?}",
        //     req.url(),
        //     req.headers(),
        //     json
        // );

        // return Ok(());

        //END TEST CODE

        let client = reqwest::Client::new();
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
