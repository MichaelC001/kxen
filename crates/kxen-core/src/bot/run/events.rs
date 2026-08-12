use serde::{Deserialize, Serialize};

use crate::agent::dcp::{ProviderNeutralPart, TurnRecord};
use crate::core::identity::ResourceId;
use crate::core::operation::OperationEvent;

use super::types::{ApprovalDecision, ApprovalRequest, ArtifactRef, InputRequest, RunSpec, ToolExecutionResult, ToolIntent, UsageSummary};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunEvent {
    Queued { spec: Box<RunSpec>, at_ms: u64 },
    Started { at_ms: u64 },
    TurnRecorded { record: TurnRecord, at_ms: u64 },
    ToolOperation { operation_id: ResourceId, event: OperationEvent<ToolIntent, ToolExecutionResult>, at_ms: u64 },
    ApprovalRequested { request: ApprovalRequest, at_ms: u64 },
    ApprovalResolved { decision: ApprovalDecision, at_ms: u64 },
    InputRequired { request: InputRequest, at_ms: u64 },
    InputBound { parts: Vec<ProviderNeutralPart>, at_ms: u64 },
    ArtifactCommitted { artifact: ArtifactRef, at_ms: u64 },
    Completed { result: Vec<ProviderNeutralPart>, usage: UsageSummary, at_ms: u64 },
    Failed { code: String, message: String, usage: UsageSummary, at_ms: u64 },
    Canceled { reason: String, usage: UsageSummary, at_ms: u64 },
    Rejected { code: String, message: String, at_ms: u64 },
    Blocked { reason: String, at_ms: u64 },
}

impl RunEvent {
    pub fn at_ms(&self) -> u64 {
        match self {
            Self::Queued { at_ms, .. }
            | Self::Started { at_ms }
            | Self::TurnRecorded { at_ms, .. }
            | Self::ToolOperation { at_ms, .. }
            | Self::ApprovalRequested { at_ms, .. }
            | Self::ApprovalResolved { at_ms, .. }
            | Self::InputRequired { at_ms, .. }
            | Self::InputBound { at_ms, .. }
            | Self::ArtifactCommitted { at_ms, .. }
            | Self::Completed { at_ms, .. }
            | Self::Failed { at_ms, .. }
            | Self::Canceled { at_ms, .. }
            | Self::Rejected { at_ms, .. }
            | Self::Blocked { at_ms, .. } => *at_ms,
        }
    }
}
