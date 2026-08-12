use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::core::identity::{ContentHash, ResourceId};

use super::definition::BotDefinition;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BotLifecycle {
    Draft,
    Active,
    Paused,
    Archived,
    Trashed,
    Blocked,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BotDraft {
    pub version: u64,
    pub definition: BotDefinition,
    pub content_hash: ContentHash,
    pub updated_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BotDefinitionRevision {
    pub revision_id: ResourceId,
    pub revision_number: u64,
    pub definition: BotDefinition,
    pub content_hash: ContentHash,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BotState {
    pub bot_id: ResourceId,
    pub lifecycle: BotLifecycle,
    pub event_version: u64,
    pub draft_version_counter: u64,
    pub draft: Option<BotDraft>,
    pub current_revision_id: Option<ResourceId>,
    pub revisions: BTreeMap<u64, BotDefinitionRevision>,
    pub blocked_reason: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

impl BotState {
    pub fn current_revision(&self) -> Option<&BotDefinitionRevision> {
        self.revisions.values().next_back()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BotSummary {
    pub bot_id: ResourceId,
    pub display_name: String,
    pub lifecycle: BotLifecycle,
    pub current_revision_id: Option<ResourceId>,
    pub current_revision_number: Option<u64>,
    pub draft_version: Option<u64>,
    pub blocked_reason: Option<String>,
    pub updated_at_ms: u64,
}

impl From<&BotState> for BotSummary {
    fn from(state: &BotState) -> Self {
        let current = state.current_revision();
        let display_name = state
            .draft
            .as_ref()
            .map(|draft| draft.definition.display_name.clone())
            .or_else(|| current.map(|revision| revision.definition.display_name.clone()))
            .unwrap_or_default();
        Self {
            bot_id: state.bot_id.clone(),
            display_name,
            lifecycle: state.lifecycle,
            current_revision_id: state.current_revision_id.clone(),
            current_revision_number: current.map(|revision| revision.revision_number),
            draft_version: state.draft.as_ref().map(|draft| draft.version),
            blocked_reason: state.blocked_reason.clone(),
            updated_at_ms: state.updated_at_ms,
        }
    }
}
