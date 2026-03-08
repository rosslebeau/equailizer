pub mod get_transactions;
pub mod update_transaction;

use crate::error::Result;
use async_trait::async_trait;
use chrono::NaiveDate;

use super::model::transaction::{Transaction, TransactionId};
pub use update_transaction::{
    SplitResponse, SplitUpdate, TransactionAndSplitUpdate, TransactionUpdate,
};

#[async_trait]
pub trait LunchMoney: Send + Sync {
    async fn get_transaction(&self, id: TransactionId) -> Result<Transaction>;
    async fn get_transactions(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<Transaction>>;
    async fn get_transactions_by_id(&self, ids: &[TransactionId]) -> Result<Vec<Transaction>>;
    async fn update_transaction(&self, update: TransactionUpdate) -> Result<()>;
    async fn update_split(&self, update: SplitUpdate) -> Result<SplitResponse>;
    async fn update_transaction_and_split(
        &self,
        update: TransactionAndSplitUpdate,
    ) -> Result<SplitResponse>;
}

pub struct LunchMoneyClient {
    pub auth_token: String,
    pub dry_run: bool,
}

#[async_trait]
impl LunchMoney for LunchMoneyClient {
    async fn get_transaction(&self, id: TransactionId) -> Result<Transaction> {
        get_transactions::get_single(self, id).await
    }

    async fn get_transactions(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<Transaction>> {
        get_transactions::get_by_date_range(self, start, end).await
    }

    async fn get_transactions_by_id(&self, ids: &[TransactionId]) -> Result<Vec<Transaction>> {
        get_transactions::get_by_ids(self, ids).await
    }

    async fn update_transaction(&self, update: TransactionUpdate) -> Result<()> {
        update_transaction::perform_update(self, update).await
    }

    async fn update_split(&self, update: SplitUpdate) -> Result<SplitResponse> {
        update_transaction::perform_split(self, update).await
    }

    async fn update_transaction_and_split(
        &self,
        update: TransactionAndSplitUpdate,
    ) -> Result<SplitResponse> {
        update_transaction::perform_update_and_split(self, update).await
    }
}
