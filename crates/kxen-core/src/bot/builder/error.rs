#[derive(Debug, thiserror::Error)]
pub enum BuilderError {
    #[error(transparent)]
    EventStore(#[from] crate::core::event_store::EventStoreError),
    #[error("BuilderSession not found: {0}")]
    NotFound(String),
    #[error("BuilderSession version conflict: expected {expected}, actual {actual}")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("Builder command rejected: {0}")]
    Rejected(String),
    #[error("Builder event invalid: {0}")]
    InvalidEvent(String),
    #[error("Builder id invalid: {0}")]
    InvalidId(String),
}
