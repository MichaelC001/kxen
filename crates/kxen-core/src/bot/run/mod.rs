//! Durable DCP source of truth for one Bot execution.

mod command;
mod decision;
mod error;
mod events;
mod projection;
mod repository;
mod types;

pub use command::RunCommand;
pub use error::RunError;
pub use repository::{RunRepository, RunWrite};
pub use types::{
    ApprovalDecision, ApprovalRequest, ArtifactRef, BotRunState, InputRequest, PermissionSnapshot, RunSpec, RunStatus, RunTrigger,
    RunTriggerKind, ToolAttempt, ToolExecutionResult, ToolIntent, ToolOperation, UsageSummary,
};

pub fn deterministic_run_id(
    source_id: &crate::core::identity::ResourceId,
    revision_id: &crate::core::identity::ResourceId,
    attempt: u32,
) -> Result<crate::core::identity::ResourceId, String> {
    crate::bot::ids::deterministic_id("brun", &[source_id.as_str(), revision_id.as_str(), &attempt.to_string()])
}

#[cfg(test)]
mod idempotency_tests;
#[cfg(test)]
mod tests;
