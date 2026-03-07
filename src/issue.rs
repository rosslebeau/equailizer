use crate::lunch_money::model::transaction::TransactionId;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Issue {
    AddTagHasChildren(TransactionId),
    SplitTagHasChildren(TransactionId),
    TransactionUpdateError(TransactionId, String),
    BatchReconcileError(String, String),
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Issue::AddTagHasChildren(txn) => {
                write!(
                    f,
                    "Transaction was tagged for batch, but it has children: {}",
                    txn
                )
            }
            Issue::SplitTagHasChildren(txn) => {
                write!(
                    f,
                    "Transaction was tagged to split, but it already has children: {}",
                    txn
                )
            }
            Issue::TransactionUpdateError(txn, e_str) => {
                write!(f, "Error when updating transaction {}: {}", txn, e_str)
            }
            Issue::BatchReconcileError(batch_id, e_str) => {
                write!(f, "Failed to reconcile batch {}: {}", batch_id, e_str)
            }
        }
    }
}
