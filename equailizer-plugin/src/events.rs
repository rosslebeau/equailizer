use crate::protocol::{BatchReconcileError, Transaction};

#[derive(Debug)]
pub struct BatchCreated {
    pub batch_id: String,
    pub total: String,
    pub transactions: Vec<Transaction>,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct BatchReconciled {
    pub batch_id: String,
    pub amount: String,
    pub settlement_credit_id: u32,
    pub settlement_debit_id: u32,
}

#[derive(Debug)]
pub struct CommandError {
    pub command: String,
    pub error: String,
}

#[derive(Debug)]
pub struct ReconcileAllComplete {
    pub reconciled_count: u32,
    pub failed_count: u32,
    pub errors: Vec<BatchReconcileError>,
}
