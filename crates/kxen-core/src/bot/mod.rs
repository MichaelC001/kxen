//! Persistent Bot definitions, immutable revisions and lifecycle management.

pub mod builder;
pub mod conversation;
mod definition;
mod error;
mod events;
pub mod executor;
mod ids;
pub mod memory;
mod projection;
mod repository;
pub mod routine;
pub mod run;
pub mod system;
pub mod tools;
mod types;

pub use definition::{
    ApprovalPolicy, BotDefinition, CommunicationPolicy, ContextPolicy, ContractSpec, FailurePolicy, MemoryPolicy, PathGrantSpec,
    ResourceAccess, ResourcePolicy, WorkspaceGrantSpec,
};
pub use error::BotError;
pub(crate) use ids::deterministic_id;
pub use repository::{BotRepository, ChangeLifecycle, CreateBot, LifecycleChange, PublishBot, ReplaceDraft};
pub use types::{BotDefinitionRevision, BotDraft, BotLifecycle, BotState, BotSummary};

#[cfg(test)]
mod tests;
