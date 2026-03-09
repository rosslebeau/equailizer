use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

// ── Host → Plugin ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
        transactions: Vec<Transaction>,
        warnings: Vec<String>,
    },
    BatchReconciled {
        batch_id: String,
        amount: String,
        settlement_credit_id: u32,
        settlement_debit_id: u32,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transaction {
    pub payee: String,
    pub amount: String,
    pub date: NaiveDate,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchReconcileError {
    pub batch_id: String,
    pub error: String,
}

// ── Plugin → Host ──

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PluginResponse {
    Ready { name: String, version: String },
    Ack,
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Serialization (host → plugin) ──

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
            transactions: vec![Transaction {
                payee: "Store A".to_string(),
                amount: "15.00".to_string(),
                date: NaiveDate::from_ymd_opt(2025, 3, 1).unwrap(),
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

    // ── Deserialization (plugin → host) ──

    #[test]
    fn deserialize_ready() {
        let json = r#"{"type": "ready", "name": "my-plugin", "version": "1.0.0"}"#;
        let resp: PluginResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            resp,
            PluginResponse::Ready {
                name: "my-plugin".to_string(),
                version: "1.0.0".to_string()
            }
        );
    }

    #[test]
    fn deserialize_ack() {
        let json = r#"{"type": "ack"}"#;
        let resp: PluginResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp, PluginResponse::Ack);
    }

    #[test]
    fn deserialize_error() {
        let json = r#"{"type": "error", "message": "something went wrong"}"#;
        let resp: PluginResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            resp,
            PluginResponse::Error {
                message: "something went wrong".to_string()
            }
        );
    }

    // ── Roundtrip (serialize → deserialize → assert equality) ──

    #[test]
    fn roundtrip_initialize() {
        let msg = PluginMessage::Initialize {
            protocol_version: 1,
            profile: "personal".to_string(),
            dry_run: true,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: PluginMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn roundtrip_batch_created() {
        let msg = PluginMessage::BatchCreated {
            batch_id: "abc-123".to_string(),
            total: "40.00".to_string(),
            transactions: vec![
                Transaction {
                    payee: "Store A".to_string(),
                    amount: "15.00".to_string(),
                    date: NaiveDate::from_ymd_opt(2025, 3, 1).unwrap(),
                    notes: Some("groceries".to_string()),
                },
                Transaction {
                    payee: "Store B".to_string(),
                    amount: "25.00".to_string(),
                    date: NaiveDate::from_ymd_opt(2025, 3, 2).unwrap(),
                    notes: None,
                },
            ],
            warnings: vec!["some warning".to_string()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: PluginMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn roundtrip_batch_reconciled() {
        let msg = PluginMessage::BatchReconciled {
            batch_id: "abc-123".to_string(),
            amount: "40.00".to_string(),
            settlement_credit_id: 50,
            settlement_debit_id: 60,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: PluginMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn roundtrip_reconcile_all_complete() {
        let msg = PluginMessage::ReconcileAllComplete {
            reconciled_count: 3,
            failed_count: 1,
            errors: vec![BatchReconcileError {
                batch_id: "xyz-789".to_string(),
                error: "settlement credit not found".to_string(),
            }],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: PluginMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn roundtrip_ready() {
        let resp = PluginResponse::Ready {
            name: "my-plugin".to_string(),
            version: "1.0.0".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: PluginResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, deserialized);
    }

    #[test]
    fn roundtrip_ack() {
        let resp = PluginResponse::Ack;
        let json = serde_json::to_string(&resp).unwrap();
        let deserialized: PluginResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, deserialized);
    }

    // ── Wire compatibility with protocol spec (docs/plugin-api.md) ──

    #[test]
    fn deserialize_batch_created_from_protocol_spec() {
        let json = r#"{
            "type": "batch_created",
            "batch_id": "abc-123",
            "total": "40.00",
            "transactions": [
                {"payee": "Store A", "amount": "15.00", "date": "2025-03-01", "notes": "groceries"},
                {"payee": "Store B", "amount": "25.00", "date": "2025-03-02", "notes": null}
            ],
            "warnings": ["Transaction was tagged for batch, but it has children: 42"]
        }"#;
        let msg: PluginMessage = serde_json::from_str(json).unwrap();
        match msg {
            PluginMessage::BatchCreated {
                batch_id,
                total,
                transactions,
                warnings,
            } => {
                assert_eq!(batch_id, "abc-123");
                assert_eq!(total, "40.00");
                assert_eq!(transactions.len(), 2);
                assert_eq!(transactions[0].payee, "Store A");
                assert_eq!(transactions[1].notes, None);
                assert_eq!(warnings.len(), 1);
            }
            _ => panic!("expected BatchCreated"),
        }
    }

    #[test]
    fn deserialize_batch_reconciled_from_protocol_spec() {
        let json = r#"{
            "type": "batch_reconciled",
            "batch_id": "abc-123",
            "amount": "40.00",
            "settlement_credit_id": 50,
            "settlement_debit_id": 60
        }"#;
        let msg: PluginMessage = serde_json::from_str(json).unwrap();
        match msg {
            PluginMessage::BatchReconciled {
                batch_id,
                amount,
                settlement_credit_id,
                settlement_debit_id,
            } => {
                assert_eq!(batch_id, "abc-123");
                assert_eq!(amount, "40.00");
                assert_eq!(settlement_credit_id, 50);
                assert_eq!(settlement_debit_id, 60);
            }
            _ => panic!("expected BatchReconciled"),
        }
    }

    #[test]
    fn deserialize_reconcile_all_complete_from_protocol_spec() {
        let json = r#"{
            "type": "reconcile_all_complete",
            "reconciled_count": 3,
            "failed_count": 1,
            "errors": [
                {"batch_id": "xyz-789", "error": "settlement credit not found for batch 'xyz-789'"}
            ]
        }"#;
        let msg: PluginMessage = serde_json::from_str(json).unwrap();
        match msg {
            PluginMessage::ReconcileAllComplete {
                reconciled_count,
                failed_count,
                errors,
            } => {
                assert_eq!(reconciled_count, 3);
                assert_eq!(failed_count, 1);
                assert_eq!(errors[0].batch_id, "xyz-789");
            }
            _ => panic!("expected ReconcileAllComplete"),
        }
    }

    // ── Response serialization ──

    #[test]
    fn serialize_ready_response() {
        let resp = PluginResponse::Ready {
            name: "test-plugin".to_string(),
            version: "0.1.0".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "ready");
        assert_eq!(value["name"], "test-plugin");
        assert_eq!(value["version"], "0.1.0");
    }

    #[test]
    fn serialize_ack_response() {
        let resp = PluginResponse::Ack;
        let json = serde_json::to_string(&resp).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "ack");
    }

    #[test]
    fn serialize_error_response() {
        let resp = PluginResponse::Error {
            message: "something went wrong".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["type"], "error");
        assert_eq!(value["message"], "something went wrong");
    }
}
