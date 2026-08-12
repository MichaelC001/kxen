use serde::{Deserialize, Serialize};

use crate::core::identity::{ContentHash, ResourceId};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptPhase {
    Prepared,
    Started,
    OutcomeKnown,
    OutcomeUnknown,
    Settled,
    CanceledBeforeStart,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRef {
    pub kind: String,
    pub id: ResourceId,
    pub content_hash: Option<ContentHash>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum OperationOutcome<O> {
    Succeeded { value: O },
    Failed { code: String, message: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SideEffectAttempt<I, O> {
    pub operation_id: ResourceId,
    pub generation: ResourceId,
    pub intent: I,
    pub intent_hash: ContentHash,
    pub phase: AttemptPhase,
    pub prepared_at_ms: u64,
    pub started_at_ms: Option<u64>,
    pub outcome: Option<OperationOutcome<O>>,
    pub evidence: Vec<EvidenceRef>,
    pub unknown_reason: Option<String>,
    pub settled_at_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperationProjection<I, O> {
    pub version: u64,
    pub attempt: Option<SideEffectAttempt<I, O>>,
}

impl<I, O> Default for OperationProjection<I, O> {
    fn default() -> Self {
        Self { version: 0, attempt: None }
    }
}

#[derive(Clone, Debug)]
pub enum OperationCommand<I, O> {
    Prepare { operation_id: ResourceId, generation: ResourceId, intent: I, at_ms: u64 },
    MarkStarted { generation: ResourceId, at_ms: u64 },
    RecordOutcome { generation: ResourceId, outcome: OperationOutcome<O>, evidence: Vec<EvidenceRef> },
    MarkOutcomeUnknown { generation: ResourceId, reason: String, evidence: Vec<EvidenceRef> },
    Settle { generation: ResourceId, at_ms: u64 },
    CancelBeforeStart { generation: ResourceId, at_ms: u64 },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OperationEvent<I, O> {
    Prepared { operation_id: ResourceId, generation: ResourceId, intent: I, intent_hash: ContentHash, at_ms: u64 },
    Started { generation: ResourceId, at_ms: u64 },
    OutcomeRecorded { generation: ResourceId, outcome: OperationOutcome<O>, evidence: Vec<EvidenceRef> },
    OutcomeMarkedUnknown { generation: ResourceId, reason: String, evidence: Vec<EvidenceRef> },
    Settled { generation: ResourceId, at_ms: u64 },
    CanceledBeforeStart { generation: ResourceId, at_ms: u64 },
}

#[derive(Clone, Debug)]
pub struct OperationDecision<I, O> {
    pub events: Vec<OperationEvent<I, O>>,
    pub duplicate: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum OperationError {
    #[error("operation codec: {0}")]
    Codec(String),
    #[error("operation id or intent collision: {0}")]
    Collision(String),
    #[error("operation has not been prepared")]
    Missing,
    #[error("operation generation is stale")]
    StaleGeneration,
    #[error("invalid operation transition from {from:?} to {to}")]
    InvalidTransition { from: AttemptPhase, to: &'static str },
    #[error("operation intent hash mismatch")]
    IntentHashMismatch,
    #[error("operation version overflow")]
    VersionOverflow,
}
