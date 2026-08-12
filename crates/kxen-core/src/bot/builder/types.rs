use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::core::identity::{ActorRef, ContentHash, ResourceId};

use crate::bot::BotDefinition;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ValidationStatus {
    Pass,
    Fail,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationFinding {
    pub code: String,
    pub status: ValidationStatus,
    pub message: String,
    pub evidence: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationReport {
    pub report_id: ResourceId,
    pub draft_hash: ContentHash,
    pub findings: Vec<ValidationFinding>,
    pub publish_eligible: bool,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionGrant {
    pub grant_id: ResourceId,
    pub draft_hash: ContentHash,
    pub permission_hash: ContentHash,
    pub reason: String,
    pub granted_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TestEvidence {
    pub run_id: ResourceId,
    pub draft_hash: ContentHash,
    pub passed: bool,
    pub criteria: BTreeMap<String, bool>,
    pub summary: String,
    pub recorded_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuilderMessage {
    pub message_id: ResourceId,
    pub actor: ActorRef,
    pub text: String,
    pub created_at_ms: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BuilderLifecycle {
    Active,
    Canceled,
    Blocked,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuilderDraft {
    pub version: u64,
    #[serde(default)]
    pub source_message_id: Option<ResourceId>,
    pub definition: BotDefinition,
    pub content_hash: ContentHash,
    pub updated_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuilderState {
    pub builder_session_id: ResourceId,
    pub bot_id: ResourceId,
    pub lifecycle: BuilderLifecycle,
    pub event_version: u64,
    pub user_goal: String,
    pub messages: Vec<BuilderMessage>,
    pub draft: Option<BuilderDraft>,
    pub grants: Vec<PermissionGrant>,
    pub reports: Vec<ValidationReport>,
    pub tests: Vec<TestEvidence>,
    pub active_test_run_id: Option<ResourceId>,
    pub blocked_reason: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

impl BuilderState {
    pub fn current_report(&self) -> Option<&ValidationReport> {
        let hash = &self.draft.as_ref()?.content_hash;
        self.reports.iter().rev().find(|report| &report.draft_hash == hash)
    }
}
