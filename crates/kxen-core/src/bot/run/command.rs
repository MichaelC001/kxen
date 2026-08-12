use crate::agent::dcp::{ProviderNeutralPart, TurnRecord};
use crate::core::identity::ResourceId;
use crate::core::operation::{EvidenceRef, OperationOutcome};

use super::types::{ApprovalDecision, ApprovalRequest, ArtifactRef, InputRequest, RunSpec, ToolExecutionResult, ToolIntent, UsageSummary};

#[derive(Clone, Debug)]
pub enum RunCommand {
    Queue {
        spec: Box<RunSpec>,
        at_ms: u64,
    },
    Start {
        at_ms: u64,
    },
    RecordTurn {
        record: TurnRecord,
        at_ms: u64,
    },
    PrepareTool {
        operation_id: ResourceId,
        generation: ResourceId,
        intent: ToolIntent,
        at_ms: u64,
    },
    MarkToolStarted {
        operation_id: ResourceId,
        generation: ResourceId,
        at_ms: u64,
    },
    RecordToolOutcome {
        operation_id: ResourceId,
        generation: ResourceId,
        outcome: OperationOutcome<ToolExecutionResult>,
        evidence: Vec<EvidenceRef>,
        at_ms: u64,
    },
    MarkToolUnknown {
        operation_id: ResourceId,
        generation: ResourceId,
        reason: String,
        evidence: Vec<EvidenceRef>,
        at_ms: u64,
    },
    SettleTool {
        operation_id: ResourceId,
        generation: ResourceId,
        at_ms: u64,
    },
    RequestApproval {
        request: ApprovalRequest,
        at_ms: u64,
    },
    ResolveApproval {
        approval_id: ResourceId,
        decision: ApprovalDecision,
        at_ms: u64,
    },
    RequireInput {
        request: InputRequest,
        at_ms: u64,
    },
    BindInput {
        request_id: ResourceId,
        parts: Vec<ProviderNeutralPart>,
        at_ms: u64,
    },
    CommitArtifact {
        artifact: ArtifactRef,
        at_ms: u64,
    },
    Complete {
        result: Vec<ProviderNeutralPart>,
        usage: UsageSummary,
        at_ms: u64,
    },
    Fail {
        code: String,
        message: String,
        usage: UsageSummary,
        at_ms: u64,
    },
    Cancel {
        reason: String,
        usage: UsageSummary,
        at_ms: u64,
    },
    Reject {
        code: String,
        message: String,
        at_ms: u64,
    },
    Block {
        reason: String,
        at_ms: u64,
    },
}
