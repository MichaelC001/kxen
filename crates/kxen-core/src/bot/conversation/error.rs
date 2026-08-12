#[derive(Debug, thiserror::Error)]
pub enum ConversationError {
    #[error(transparent)]
    EventStore(#[from] crate::core::event_store::EventStoreError),
    #[error(transparent)]
    Delivery(#[from] crate::core::delivery::DeliveryError),
    #[error("conversation not found: {0}")]
    NotFound(String),
    #[error("conversation version conflict: expected {expected}, actual {actual}")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("conversation command rejected: {0}")]
    Rejected(String),
    #[error("conversation event invalid: {0}")]
    InvalidEvent(String),
    #[error("conversation id invalid: {0}")]
    InvalidId(String),
}
