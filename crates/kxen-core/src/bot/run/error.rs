#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    EventStore(#[from] crate::core::event_store::EventStoreError),
    #[error("BotRun not found: {0}")]
    NotFound(String),
    #[error("BotRun version conflict: expected {expected}, actual {actual}")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("BotRun transition rejected: {0}")]
    Transition(String),
    #[error("BotRun event is invalid: {0}")]
    InvalidEvent(String),
    #[error("BotRun operation: {0}")]
    Operation(#[from] crate::core::operation::OperationError),
    #[error("BotRun id is invalid: {0}")]
    InvalidId(String),
}
