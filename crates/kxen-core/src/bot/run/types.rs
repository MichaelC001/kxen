use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::agent::capability::CapabilitySet;
use crate::agent::dcp::{ProviderNeutralPart, TurnRecord};
use crate::agent::runtime::ExecutionBudget;
use crate::core::identity::{ContentHash, ResourceId};
use crate::core::operation::{OperationProjection, SideEffectAttempt};

use crate::bot::{ApprovalPolicy, ResourcePolicy};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunTriggerKind {
    Manual,
    HumanMessage,
    Routine,
    BotRequest,
    GroupMention,
    BuilderTest,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunTrigger {
    pub kind: RunTriggerKind,
    pub source_id: Option<ResourceId>,
    pub occurrence_id: Option<ResourceId>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionSnapshot {
    pub capabilities: CapabilitySet,
    pub resources: ResourcePolicy,
    pub approval: ApprovalPolicy,
    pub budget: ExecutionBudget,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunSpec {
    pub run_id: ResourceId,
    pub bot_id: ResourceId,
    pub revision_id: ResourceId,
    pub revision_hash: ContentHash,
    pub mrm_role: ResourceId,
    pub trigger: RunTrigger,
    pub input: Vec<ProviderNeutralPart>,
    pub conversation_id: Option<ResourceId>,
    pub task_id: Option<ResourceId>,
    pub permission: PermissionSnapshot,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Queued,
    Running,
    ApprovalRequired,
    InputRequired,
    Completed,
    Failed,
    Canceled,
    Rejected,
    Blocked,
}

impl RunStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Canceled | Self::Rejected | Self::Blocked)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolIntent {
    pub call_id: ResourceId,
    pub capability_id: ResourceId,
    pub arguments_json: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolExecutionResult {
    pub output: String,
    pub is_error: bool,
}

pub type ToolAttempt = SideEffectAttempt<ToolIntent, ToolExecutionResult>;
pub type ToolOperation = OperationProjection<ToolIntent, ToolExecutionResult>;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalRequest {
    pub approval_id: ResourceId,
    pub operation_id: ResourceId,
    pub summary: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Approved,
    Denied,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InputRequest {
    pub request_id: ResourceId,
    pub prompt: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UsageSummary {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub tool_calls: u32,
    pub turns: u32,
    pub wall_clock_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    pub artifact_id: ResourceId,
    pub display_name: String,
    pub media_type: String,
    pub content_hash: ContentHash,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BotRunState {
    pub spec: RunSpec,
    pub status: RunStatus,
    pub event_version: u64,
    pub turns: Vec<TurnRecord>,
    pub tool_operations: BTreeMap<ResourceId, ToolOperation>,
    pub approved_operations: std::collections::BTreeSet<ResourceId>,
    pub approval: Option<ApprovalRequest>,
    pub input_request: Option<InputRequest>,
    pub bound_inputs: Vec<ProviderNeutralPart>,
    pub artifacts: Vec<ArtifactRef>,
    pub usage: UsageSummary,
    pub result: Vec<ProviderNeutralPart>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}
