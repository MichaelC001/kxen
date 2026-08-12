use crate::agent::dcp::ProviderNeutralPart;
use crate::agent::runtime::ExecutionBudget;
use crate::core::identity::{ActorRef, IdempotencyKey, ResourceId, TraceContext};

use crate::bot::conversation::{ConversationCommand, Message};
use crate::bot::run::RunTrigger;

pub struct QueueRun {
    pub run_id: ResourceId,
    pub bot_id: ResourceId,
    pub revision_id: Option<ResourceId>,
    pub trigger: RunTrigger,
    pub input: Vec<ProviderNeutralPart>,
    pub conversation_id: Option<ResourceId>,
    pub task_id: Option<ResourceId>,
    pub budget_override: Option<ExecutionBudget>,
    pub actor: ActorRef,
    pub trace: TraceContext,
    pub idempotency_key: IdempotencyKey,
    pub at_ms: u64,
}

pub struct PostConversation {
    pub conversation_id: ResourceId,
    pub expected_version: u64,
    pub actor: ActorRef,
    pub message: Message,
    pub task: Option<crate::bot::conversation::NewTask>,
    pub trace: TraceContext,
    pub idempotency_key: IdempotencyKey,
    pub at_ms: u64,
}

pub struct ConversationMutation {
    pub conversation_id: ResourceId,
    pub expected_version: u64,
    pub actor: ActorRef,
    pub command: ConversationCommand,
    pub trace: TraceContext,
    pub idempotency_key: IdempotencyKey,
}

pub struct DispatchReceipt {
    pub conversation_id: ResourceId,
    pub delivery_id: ResourceId,
    pub run: crate::bot::run::BotRunState,
}

#[derive(Clone, Debug, Default)]
pub struct RoutineTickReport {
    pub queued_run_ids: Vec<ResourceId>,
    pub skipped_occurrences: usize,
    pub errors: Vec<String>,
}
