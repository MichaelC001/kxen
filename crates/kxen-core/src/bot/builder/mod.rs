//! Restricted Bot Build Agent state and deterministic validation.

pub mod agent;
mod command;
mod decision;
mod error;
mod events;
mod projection;
mod repository;
mod types;
mod validator;

pub use command::BuilderCommand;
pub use error::BuilderError;
pub use repository::{BuilderRepository, BuilderWrite};
pub use types::{
    BuilderDraft, BuilderLifecycle, BuilderMessage, BuilderState, PermissionGrant, TestEvidence, ValidationFinding, ValidationReport,
    ValidationStatus,
};
pub use validator::{ValidationContext, permission_hash, validate};

pub const BUILDER_MRM_ROLE: &str = "bot_builder";
pub const BUILDER_CAPABILITIES: &[&str] = &["bot_catalog_read", "bot_draft_patch", "bot_validate", "bot_test_run", "bot_test_inspect"];

#[cfg(test)]
mod tests;
