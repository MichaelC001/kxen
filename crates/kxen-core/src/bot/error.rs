#[derive(Debug, thiserror::Error)]
pub enum BotError {
    #[error("bot storage IO: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    EventStore(#[from] crate::core::event_store::EventStoreError),
    #[error("bot not found: {0}")]
    NotFound(String),
    #[error("bot already exists: {0}")]
    AlreadyExists(String),
    #[error("bot definition is invalid: {0}")]
    InvalidDefinition(String),
    #[error("bot version conflict: {0}")]
    VersionConflict(String),
    #[error("bot lifecycle rejects operation: {0}")]
    LifecycleRejected(String),
    #[error("bot event is invalid: {0}")]
    InvalidEvent(String),
}
