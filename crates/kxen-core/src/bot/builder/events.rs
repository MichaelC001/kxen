use serde::{Deserialize, Serialize};

use crate::core::identity::ResourceId;

use super::types::{BuilderDraft, BuilderMessage, PermissionGrant, TestEvidence, ValidationReport};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BuilderEvent {
    Started { builder_session_id: ResourceId, bot_id: ResourceId, user_goal: String, at_ms: u64 },
    MessageAppended { message: BuilderMessage, at_ms: u64 },
    DraftReplaced { draft: Box<BuilderDraft>, at_ms: u64 },
    PermissionGranted { grant: PermissionGrant, at_ms: u64 },
    ValidationRecorded { report: ValidationReport, at_ms: u64 },
    TestRunLinked { run_id: ResourceId, draft_hash: crate::core::identity::ContentHash, at_ms: u64 },
    TestEvidenceRecorded { evidence: TestEvidence, at_ms: u64 },
    Canceled { at_ms: u64 },
    Blocked { reason: String, at_ms: u64 },
}

impl BuilderEvent {
    pub fn at_ms(&self) -> u64 {
        match self {
            Self::Started { at_ms, .. }
            | Self::MessageAppended { at_ms, .. }
            | Self::DraftReplaced { at_ms, .. }
            | Self::PermissionGranted { at_ms, .. }
            | Self::ValidationRecorded { at_ms, .. }
            | Self::TestRunLinked { at_ms, .. }
            | Self::TestEvidenceRecorded { at_ms, .. }
            | Self::Canceled { at_ms }
            | Self::Blocked { at_ms, .. } => *at_ms,
        }
    }
}
