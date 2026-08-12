use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::agent::runtime::ExecutionBudget;
use crate::core::delivery::DeliveryProjection;
use crate::core::identity::{ActorRef, ResourceId};

use crate::bot::run::ArtifactRef;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationKind {
    HumanBot,
    BotDirect,
    BotGroup,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationLifecycle {
    Active,
    Paused,
    Archived,
    Blocked,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BotParticipant {
    pub bot_id: ResourceId,
    pub joined_at_seq: u64,
    pub history_visible_from_seq: u64,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Instruction,
    Request,
    Response,
    Notice,
    Status,
    Artifact,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MessagePart {
    Text { text: String },
    Data { schema_id: ResourceId, fields: BTreeMap<String, String> },
    ArtifactRef { artifact: ArtifactRef },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub message_id: ResourceId,
    pub conversation_id: ResourceId,
    pub actor: ActorRef,
    pub kind: MessageKind,
    pub parts: Vec<MessagePart>,
    pub mentions: BTreeSet<ResourceId>,
    pub everyone: bool,
    pub target_bot_id: Option<ResourceId>,
    pub reply_to_message_id: Option<ResourceId>,
    pub task_id: Option<ResourceId>,
    pub origin_run_id: Option<ResourceId>,
    pub causation_id: Option<ResourceId>,
    pub correlation_id: Option<ResourceId>,
    pub delegation_depth: u16,
    pub hop_count: u16,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MessageDelivery {
    pub message_id: ResourceId,
    pub task_id: Option<ResourceId>,
    pub delegation_depth: u16,
    pub hop_count: u16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Submitted,
    Working,
    InputRequired,
    ApprovalRequired,
    Completed,
    Failed,
    Canceled,
    Rejected,
    Blocked,
}

impl TaskStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Canceled | Self::Rejected | Self::Blocked)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CollaborationTask {
    pub task_id: ResourceId,
    pub conversation_id: ResourceId,
    pub originator: ActorRef,
    pub owner_bot_id: ResourceId,
    pub title: String,
    pub input: Vec<MessagePart>,
    pub expected_output: String,
    pub status: TaskStatus,
    pub result: Vec<MessagePart>,
    pub origin_run_id: Option<ResourceId>,
    pub parent_task_id: Option<ResourceId>,
    pub delegation_depth: u16,
    pub hop_count: u16,
    pub budget: ExecutionBudget,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NewTask {
    pub task_id: ResourceId,
    pub owner_bot_id: ResourceId,
    pub title: String,
    pub input: Vec<MessagePart>,
    pub expected_output: String,
    pub parent_task_id: Option<ResourceId>,
    pub budget: ExecutionBudget,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationState {
    pub conversation_id: ResourceId,
    pub kind: ConversationKind,
    pub lifecycle: ConversationLifecycle,
    pub event_version: u64,
    pub members: BTreeMap<ResourceId, BotParticipant>,
    pub moderator_bot_id: Option<ResourceId>,
    pub blocked_reason: Option<String>,
    pub messages: Vec<Message>,
    pub message_sequences: BTreeMap<ResourceId, u64>,
    pub deliveries: DeliveryProjection<MessageDelivery>,
    pub delivery_runs: BTreeMap<ResourceId, ResourceId>,
    pub tasks: BTreeMap<ResourceId, CollaborationTask>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

impl ConversationState {
    pub fn active_members(&self) -> impl Iterator<Item = &ResourceId> {
        self.members.values().filter(|member| member.active).map(|member| &member.bot_id)
    }
}
