pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // ── Domain / business logic ──
    #[error("start date cannot be after end date")]
    InvalidDateRange,

    #[error("batch '{0}' is already reconciled")]
    BatchAlreadyReconciled(String),

    #[error("no creditor transactions found for reconciliation")]
    NoTransactionsFound,

    #[error("settlement {side} not found for batch '{batch_id}'")]
    SettlementNotFound {
        side: &'static str,
        batch_id: String,
    },

    #[error("failed to reconcile batch '{batch_id}': {source}")]
    BatchReconcile {
        batch_id: String,
        source: Box<Error>,
    },

    // ── Lunch Money API ──
    #[error("{0}")]
    Api(String),

    #[error(transparent)]
    Http(#[from] reqwest::Error),

    // ── Persistence ──
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    // ── Notifications ──
    #[error("{0}")]
    Notification(String),
}
