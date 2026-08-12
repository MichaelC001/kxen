use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::agent::dcp::ProviderNeutralPart;
use crate::agent::runtime::ExecutionBudget;
use crate::core::identity::ResourceId;
use crate::core::scheduler::ScheduleSpec;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextMode {
    Isolated,
    ContinueConversation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RevisionPolicy {
    FollowCurrent,
    Pinned { revision_id: ResourceId },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoutineDefinition {
    pub bot_id: ResourceId,
    pub name: String,
    pub schedule: ScheduleSpec,
    pub context_mode: ContextMode,
    pub target_conversation_id: Option<ResourceId>,
    pub input: Vec<ProviderNeutralPart>,
    pub budget_override: Option<ExecutionBudget>,
    pub revision_policy: RevisionPolicy,
    pub failure_threshold: u8,
}

impl RoutineDefinition {
    pub fn validate(&self) -> Result<(), String> {
        self.schedule.validate().map_err(|error| error.to_string())?;
        if self.name.trim().is_empty() || self.input.is_empty() || self.failure_threshold == 0 {
            return Err("Routine requires name, input and positive failure threshold".into());
        }
        if self.context_mode == ContextMode::ContinueConversation && self.target_conversation_id.is_none() {
            return Err("continue_conversation requires target Conversation".into());
        }
        if let Some(budget) = &self.budget_override {
            budget.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutineLifecycle {
    Active,
    Paused,
    Trashed,
    Blocked,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OccurrenceStatus {
    Recorded,
    Skipped,
    RunLinked,
    Completed,
    Failed,
    Blocked,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoutineOccurrence {
    pub occurrence_id: ResourceId,
    pub scheduled_at_ms: u64,
    pub observed_at_ms: u64,
    pub missed_before: u32,
    pub manual: bool,
    pub status: OccurrenceStatus,
    pub resolved_revision_id: Option<ResourceId>,
    pub run_id: Option<ResourceId>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoutineState {
    pub routine_id: ResourceId,
    pub definition: RoutineDefinition,
    pub lifecycle: RoutineLifecycle,
    pub event_version: u64,
    pub last_observed_at_ms: u64,
    pub next_scheduled_at_ms: Option<u64>,
    pub occurrences: BTreeMap<ResourceId, RoutineOccurrence>,
    pub consecutive_failures: u8,
    pub blocked_reason: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}
