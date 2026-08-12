use serde::{Deserialize, Serialize};

use crate::core::identity::ResourceId;

use super::types::{BotDefinitionRevision, BotDraft};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BotEvent {
    Created { bot_id: ResourceId, draft: BotDraft, at_ms: u64 },
    DraftReplaced { draft: BotDraft, at_ms: u64 },
    RevisionPublished { revision: BotDefinitionRevision, at_ms: u64 },
    Paused { at_ms: u64 },
    Resumed { at_ms: u64 },
    Archived { at_ms: u64 },
    Trashed { at_ms: u64 },
    Restored { at_ms: u64 },
    Blocked { reason: String, at_ms: u64 },
    RecoveryCleared { at_ms: u64 },
}

impl BotEvent {
    pub fn at_ms(&self) -> u64 {
        match self {
            Self::Created { at_ms, .. }
            | Self::DraftReplaced { at_ms, .. }
            | Self::RevisionPublished { at_ms, .. }
            | Self::Paused { at_ms }
            | Self::Resumed { at_ms }
            | Self::Archived { at_ms }
            | Self::Trashed { at_ms }
            | Self::Restored { at_ms }
            | Self::Blocked { at_ms, .. }
            | Self::RecoveryCleared { at_ms } => *at_ms,
        }
    }
}
