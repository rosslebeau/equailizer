use anyhow::Result;
use async_trait::async_trait;
use chrono::NaiveDate;
use std::sync::Mutex;

use equailizer::lunch_money::api::update_transaction::{
    SplitResponse, SplitUpdate, TransactionAndSplitUpdate, TransactionUpdate,
};
use equailizer::lunch_money::api::LunchMoney;
use equailizer::lunch_money::model::transaction::{Transaction, TransactionId};
use equailizer::persist::{Batch, Persistence};
use equailizer::usd::USD;
use equailizer::email::{EmailSender, Txn};

// ── MockLunchMoney ──────────────────────────────────────────────────────

/// A mock Lunch Money client for testing.
/// Set up `transactions` to control what `get_*` methods return.
/// `next_split_ids` controls what `update_split`/`update_transaction_and_split` return.
/// All calls are recorded for assertion.
pub struct MockLunchMoney {
    pub transactions: Vec<Transaction>,
    pub next_split_ids: Mutex<Vec<Vec<TransactionId>>>,
    pub updates_received: Mutex<Vec<TransactionUpdate>>,
    pub splits_received: Mutex<Vec<SplitUpdate>>,
    pub update_and_splits_received: Mutex<Vec<TransactionAndSplitUpdate>>,
}

impl MockLunchMoney {
    pub fn new(transactions: Vec<Transaction>) -> Self {
        Self {
            transactions,
            next_split_ids: Mutex::new(vec![]),
            updates_received: Mutex::new(vec![]),
            splits_received: Mutex::new(vec![]),
            update_and_splits_received: Mutex::new(vec![]),
        }
    }

    /// Set the split IDs that will be returned by successive split/update_and_split calls.
    pub fn with_split_ids(self, split_ids: Vec<Vec<TransactionId>>) -> Self {
        *self.next_split_ids.lock().unwrap() = split_ids;
        self
    }
}

#[async_trait]
impl LunchMoney for MockLunchMoney {
    async fn get_transaction(&self, id: TransactionId) -> Result<Transaction> {
        self.transactions
            .iter()
            .find(|t| t.id == id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("mock: transaction {} not found", id))
    }

    async fn get_transactions(
        &self,
        _start: NaiveDate,
        _end: NaiveDate,
    ) -> Result<Vec<Transaction>> {
        Ok(self.transactions.clone())
    }

    async fn get_transactions_by_id(&self, ids: &[TransactionId]) -> Result<Vec<Transaction>> {
        let mut result = vec![];
        for id in ids {
            result.push(self.get_transaction(*id).await?);
        }
        Ok(result)
    }

    async fn update_transaction(&self, update: TransactionUpdate) -> Result<()> {
        self.updates_received.lock().unwrap().push(update);
        Ok(())
    }

    async fn update_split(&self, update: SplitUpdate) -> Result<SplitResponse> {
        self.splits_received.lock().unwrap().push(update);
        let ids = self
            .next_split_ids
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| vec![100, 101]);
        Ok(SplitResponse { split_ids: ids })
    }

    async fn update_transaction_and_split(
        &self,
        update: TransactionAndSplitUpdate,
    ) -> Result<SplitResponse> {
        self.update_and_splits_received
            .lock()
            .unwrap()
            .push(update);
        let ids = self
            .next_split_ids
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| vec![100, 101]);
        Ok(SplitResponse { split_ids: ids })
    }
}

// ── InMemoryPersistence ─────────────────────────────────────────────────

/// An in-memory persistence implementation for testing.
pub struct InMemoryPersistence {
    batches: Mutex<Vec<Batch>>,
}

impl InMemoryPersistence {
    pub fn new() -> Self {
        Self {
            batches: Mutex::new(vec![]),
        }
    }

    pub fn with_batches(batches: Vec<Batch>) -> Self {
        Self {
            batches: Mutex::new(batches),
        }
    }

    pub fn saved_batches(&self) -> Vec<Batch> {
        self.batches.lock().unwrap().clone()
    }
}

impl Persistence for InMemoryPersistence {
    fn save_batch(&self, batch: &Batch) -> Result<()> {
        let mut batches = self.batches.lock().unwrap();
        // Replace if exists, otherwise insert
        if let Some(pos) = batches.iter().position(|b| b.id == batch.id) {
            batches[pos] = Batch {
                id: batch.id.clone(),
                amount: batch.amount,
                transaction_ids: batch.transaction_ids.clone(),
                reconciliation: batch.reconciliation.as_ref().map(|r| {
                    equailizer::persist::Settlement {
                        settlement_credit_id: r.settlement_credit_id,
                        settlement_debit_id: r.settlement_debit_id,
                    }
                }),
            };
        } else {
            batches.push(Batch {
                id: batch.id.clone(),
                amount: batch.amount,
                transaction_ids: batch.transaction_ids.clone(),
                reconciliation: batch.reconciliation.as_ref().map(|r| {
                    equailizer::persist::Settlement {
                        settlement_credit_id: r.settlement_credit_id,
                        settlement_debit_id: r.settlement_debit_id,
                    }
                }),
            });
        }
        Ok(())
    }

    fn get_batch(&self, batch_name: &str) -> Result<Batch> {
        let batches = self.batches.lock().unwrap();
        batches
            .iter()
            .find(|b| b.id == batch_name)
            .map(|b| Batch {
                id: b.id.clone(),
                amount: b.amount,
                transaction_ids: b.transaction_ids.clone(),
                reconciliation: b.reconciliation.as_ref().map(|r| {
                    equailizer::persist::Settlement {
                        settlement_credit_id: r.settlement_credit_id,
                        settlement_debit_id: r.settlement_debit_id,
                    }
                }),
            })
            .ok_or_else(|| anyhow::anyhow!("mock: batch '{}' not found", batch_name))
    }

    fn all_batches(&self) -> Result<Vec<Batch>> {
        let batches = self.batches.lock().unwrap();
        Ok(batches
            .iter()
            .map(|b| Batch {
                id: b.id.clone(),
                amount: b.amount,
                transaction_ids: b.transaction_ids.clone(),
                reconciliation: b.reconciliation.as_ref().map(|r| {
                    equailizer::persist::Settlement {
                        settlement_credit_id: r.settlement_credit_id,
                        settlement_debit_id: r.settlement_debit_id,
                    }
                }),
            })
            .collect())
    }

    fn unreconciled_batches(&self) -> Result<Vec<Batch>> {
        Ok(self
            .all_batches()?
            .into_iter()
            .filter(|b| b.reconciliation.is_none())
            .collect())
    }
}

// ── RecordingEmailSender ────────────────────────────────────────────────

/// Records email send calls for assertion. Does not send real emails.
pub struct RecordingEmailSender {
    pub calls: Mutex<Vec<EmailSendCall>>,
}

pub struct EmailSendCall {
    pub batch_id: String,
    pub total: USD,
    pub txn_count: usize,
    pub warnings: Vec<String>,
}

impl RecordingEmailSender {
    pub fn new() -> Self {
        Self {
            calls: Mutex::new(vec![]),
        }
    }

    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

#[async_trait]
impl EmailSender for RecordingEmailSender {
    async fn send_batch_emails(
        &self,
        batch_id: &str,
        total: &USD,
        txns: &[Txn],
        warnings: Vec<String>,
    ) -> Result<()> {
        self.calls.lock().unwrap().push(EmailSendCall {
            batch_id: batch_id.to_string(),
            total: *total,
            txn_count: txns.len(),
            warnings,
        });
        Ok(())
    }
}
