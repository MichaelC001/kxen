use crate::core::identity::{ContentHash, ResourceId};

use crate::bot::BotDefinition;

use super::types::{BuilderMessage, PermissionGrant, TestEvidence, ValidationReport};

#[derive(Clone, Debug)]
pub enum BuilderCommand {
    Start { builder_session_id: ResourceId, bot_id: ResourceId, user_goal: String, at_ms: u64 },
    AppendMessage { message: BuilderMessage, at_ms: u64 },
    ReplaceDraft { expected_draft_version: u64, source_message_id: Option<ResourceId>, definition: Box<BotDefinition>, at_ms: u64 },
    RecordGrant { grant: PermissionGrant, at_ms: u64 },
    RecordValidation { report: ValidationReport, at_ms: u64 },
    LinkTestRun { run_id: ResourceId, draft_hash: ContentHash, at_ms: u64 },
    RecordTestEvidence { evidence: TestEvidence, at_ms: u64 },
    Cancel { at_ms: u64 },
    Block { reason: String, at_ms: u64 },
}
