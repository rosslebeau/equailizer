use async_trait::async_trait;
use chrono::NaiveDate;
use std::sync::Mutex;

use equailizer::error::{Error, Result};
use equailizer::lunch_money::api::update_transaction::{
    SplitResponse, SplitUpdate, TransactionAndSplitUpdate, TransactionUpdate,
};
use equailizer::lunch_money::api::LunchMoney;
use equailizer::lunch_money::model::transaction::{Transaction, TransactionId};
use equailizer::persist::{Batch, Persistence};
use equailizer::usd::USD;
use equailizer::email::{BatchNotifier, Txn};

// ── MockLunchMoney ──────────────────────────────────────────────────────

/// A mock Lunch Money client for testing.
/// Set up `transactions` to control what `get_*` methods return.
/// `next_split_ids` controls what `update_split`/`update_transaction_and_split` return.
/// All calls are recorded for assertion.
pub struct MockLunchMoney {
    pub transactions: Vec<Transaction>,
    pub next_split_ids: Mutex<Vec<Vec<TransactionId>>>,
    pub fail_update_for_ids: Mutex<Vec<TransactionId>>,
    pub updates_received: Mutex<Vec<TransactionUpdate>>,
    pub splits_received: Mutex<Vec<SplitUpdate>>,
    pub update_and_splits_received: Mutex<Vec<TransactionAndSplitUpdate>>,
}

impl MockLunchMoney {
    pub fn new(transactions: Vec<Transaction>) -> Self {
        Self {
            transactions,
            next_split_ids: Mutex::new(vec![]),
            fail_update_for_ids: Mutex::new(vec![]),
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

    /// Make `update_transaction` return an error for the given transaction IDs.
    pub fn with_failing_updates(self, ids: Vec<TransactionId>) -> Self {
        *self.fail_update_for_ids.lock().unwrap() = ids;
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
            .ok_or_else(|| Error::Api(format!("mock: transaction {} not found", id)))
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
        let should_fail = self.fail_update_for_ids.lock().unwrap().contains(&update.0);
        self.updates_received.lock().unwrap().push(update);
        if should_fail {
            return Err(Error::Api("mock update failure".to_string()));
        }
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
            .ok_or_else(|| {
                Error::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("mock: batch '{}' not found", batch_name),
                ))
            })
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

// ── RecordingBatchNotifier ──────────────────────────────────────────────

/// Records batch notification calls for assertion. Does not send real emails.
pub struct RecordingBatchNotifier {
    pub calls: Mutex<Vec<BatchNotification>>,
}

pub struct BatchNotification {
    pub batch_id: String,
    pub total: USD,
    pub txn_count: usize,
    pub warnings: Vec<String>,
}

impl RecordingBatchNotifier {
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
impl BatchNotifier for RecordingBatchNotifier {
    async fn send_batch_notification(
        &self,
        batch_id: &str,
        total: &USD,
        txns: &[Txn],
        warnings: Vec<String>,
    ) -> Result<()> {
        self.calls.lock().unwrap().push(BatchNotification {
            batch_id: batch_id.to_string(),
            total: *total,
            txn_count: txns.len(),
            warnings,
        });
        Ok(())
    }
}
