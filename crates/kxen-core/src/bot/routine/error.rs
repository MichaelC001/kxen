#[derive(Debug, thiserror::Error)]
pub enum RoutineError {
    #[error(transparent)]
    EventStore(#[from] crate::core::event_store::EventStoreError),
    #[error("Routine not found: {0}")]
    NotFound(String),
    #[error("Routine version conflict: expected {expected}, actual {actual}")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("Routine command rejected: {0}")]
    Rejected(String),
    #[error("Routine event invalid: {0}")]
    InvalidEvent(String),
    #[error("Routine id invalid: {0}")]
    InvalidId(String),
}
