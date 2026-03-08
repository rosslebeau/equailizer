use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::email::Txn;
use crate::error::Error;
use crate::lunch_money::model::transaction::TransactionId;
use crate::persist::Batch;
use crate::usd::USD;

// ── Host → Plugin ──

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PluginMessage {
    Initialize {
        protocol_version: u32,
        profile: String,
        dry_run: bool,
    },
    BatchCreated {
        batch_id: String,
        total: String,
        transactions: Vec<PluginTransaction>,
        warnings: Vec<String>,
    },
    BatchReconciled {
        batch_id: String,
        amount: String,
        settlement_credit_id: TransactionId,
        settlement_debit_id: TransactionId,
    },
    CommandError {
        command: String,
        error: String,
    },
    ReconcileAllComplete {
        reconciled_count: u32,
        failed_count: u32,
        errors: Vec<BatchReconcileError>,
    },
    Shutdown,
}

#[derive(Debug, Serialize)]
pub struct PluginTransaction {
    pub payee: String,
    pub amount: String,
    pub date: NaiveDate,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BatchReconcileError {
    pub batch_id: String,
    pub error: String,
}

// ── Plugin → Host ──

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PluginResponse {
    Ready { name: String, version: String },
    Ack,
    Error { message: String },
}

// ── Conversions ──

impl From<&Txn> for PluginTransaction {
    fn from(txn: &Txn) -> Self {
        Self {
            payee: txn.payee.clone(),
            amount: txn.amount.to_string(),
            date: txn.date,
            notes: txn.notes.clone(),
        }
    }
}

impl PluginMessage {
    pub fn batch_created(
        batch_id: &str,
        total: &USD,
        txns: &[Txn],
        warnings: &[String],
    ) -> Self {
        Self::BatchCreated {
            batch_id: batch_id.to_string(),
            total: total.to_string(),
            transactions: txns.iter().map(PluginTransaction::from).collect(),
            warnings: warnings.to_vec(),
        }
    }

    pub fn batch_reconciled(batch: &Batch, settlement_credit_id: TransactionId, settlement_debit_id: TransactionId) -> Self {
        Self::BatchReconciled {
            batch_id: batch.id.clone(),
            amount: batch.amount.to_string(),
            settlement_credit_id,
            settlement_debit_id,
        }
    }

    pub fn reconcile_all_complete(reconciled_count: u32, errors: &[Error]) -> Self {
        Self::ReconcileAllComplete {
            reconciled_count,
            failed_count: errors.len() as u32,
            errors: errors
                .iter()
                .map(|e| match e {
                    Error::BatchReconcile { batch_id, source } => BatchReconcileError {
                        batch_id: batch_id.clone(),
                        error: source.to_string(),
                    },
                    other => BatchReconcileError {
                        batch_id: String::new(),
                        error: other.to_string(),
                    },
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_initialize() {
        let msg = PluginMessage::Initialize {
            protocol_version: 1,
            profile: "personal".to_string(),
            dry_run: false,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "initialize");
        assert_eq!(value["protocol_version"], 1);
        assert_eq!(value["profile"], "personal");
        assert_eq!(value["dry_run"], false);
    }

    #[test]
    fn serialize_batch_created() {
        let msg = PluginMessage::BatchCreated {
            batch_id: "abc-123".to_string(),
            total: "40.00".to_string(),
            transactions: vec![PluginTransaction {
                payee: "Store A".to_string(),
                amount: "15.00".to_string(),
                date: chrono::NaiveDate::from_ymd_opt(2025, 3, 1).unwrap(),
                notes: Some("groceries".to_string()),
            }],
            warnings: vec!["some warning".to_string()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "batch_created");
        assert_eq!(value["batch_id"], "abc-123");
        assert_eq!(value["total"], "40.00");
        assert_eq!(value["transactions"][0]["payee"], "Store A");
        assert_eq!(value["transactions"][0]["amount"], "15.00");
        assert_eq!(value["transactions"][0]["notes"], "groceries");
        assert_eq!(value["warnings"][0], "some warning");
    }

    #[test]
    fn serialize_batch_reconciled() {
        let msg = PluginMessage::BatchReconciled {
            batch_id: "abc-123".to_string(),
            amount: "40.00".to_string(),
            settlement_credit_id: 50,
            settlement_debit_id: 60,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "batch_reconciled");
        assert_eq!(value["settlement_credit_id"], 50);
        assert_eq!(value["settlement_debit_id"], 60);
    }

    #[test]
    fn serialize_command_error() {
        let msg = PluginMessage::CommandError {
            command: "create-batch".to_string(),
            error: "start date cannot be after end date".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "command_error");
        assert_eq!(value["command"], "create-batch");
    }

    #[test]
    fn serialize_reconcile_all_complete() {
        let msg = PluginMessage::ReconcileAllComplete {
            reconciled_count: 3,
            failed_count: 1,
            errors: vec![BatchReconcileError {
                batch_id: "xyz-789".to_string(),
                error: "settlement credit not found".to_string(),
            }],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "reconcile_all_complete");
        assert_eq!(value["reconciled_count"], 3);
        assert_eq!(value["failed_count"], 1);
        assert_eq!(value["errors"][0]["batch_id"], "xyz-789");
    }

    #[test]
    fn serialize_shutdown() {
        let msg = PluginMessage::Shutdown;
        let json = serde_json::to_string(&msg).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "shutdown");
    }

    #[test]
    fn deserialize_ready() {
        let json = r#"{"type": "ready", "name": "my-plugin", "version": "1.0.0"}"#;
        let resp: PluginResponse = serde_json::from_str(json).unwrap();
        match resp {
            PluginResponse::Ready { name, version } => {
                assert_eq!(name, "my-plugin");
                assert_eq!(version, "1.0.0");
            }
            _ => panic!("expected Ready"),
        }
    }

    #[test]
    fn deserialize_ack() {
        let json = r#"{"type": "ack"}"#;
        let resp: PluginResponse = serde_json::from_str(json).unwrap();
        assert!(matches!(resp, PluginResponse::Ack));
    }

    #[test]
    fn deserialize_error() {
        let json = r#"{"type": "error", "message": "something went wrong"}"#;
        let resp: PluginResponse = serde_json::from_str(json).unwrap();
        match resp {
            PluginResponse::Error { message } => {
                assert_eq!(message, "something went wrong");
            }
            _ => panic!("expected Error"),
        }
    }
}
