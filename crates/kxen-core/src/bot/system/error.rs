#[derive(Debug, thiserror::Error)]
pub enum BotSystemError {
    #[error(transparent)]
    Bot(#[from] crate::bot::BotError),
    #[error(transparent)]
    Run(#[from] crate::bot::run::RunError),
    #[error(transparent)]
    Conversation(#[from] crate::bot::conversation::ConversationError),
    #[error(transparent)]
    Routine(#[from] crate::bot::routine::RoutineError),
    #[error(transparent)]
    Builder(#[from] crate::bot::builder::BuilderError),
    #[error(transparent)]
    Memory(#[from] crate::bot::memory::MemoryError),
    #[error(transparent)]
    Artifact(#[from] crate::core::artifact::ArtifactError),
    #[error(transparent)]
    Recovery(#[from] crate::core::recovery::RecoveryError),
    #[error("Bot system admission rejected: {0}")]
    Rejected(String),
    #[error("Bot system id invalid: {0}")]
    InvalidId(String),
}
