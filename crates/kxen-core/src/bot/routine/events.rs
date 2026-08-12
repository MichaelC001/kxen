use serde::{Deserialize, Serialize};

use crate::core::identity::ResourceId;

use super::types::{RoutineDefinition, RoutineOccurrence};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RoutineEvent {
    Created { routine_id: ResourceId, definition: RoutineDefinition, next_scheduled_at_ms: Option<u64>, at_ms: u64 },
    Updated { definition: RoutineDefinition, next_scheduled_at_ms: Option<u64>, at_ms: u64 },
    Paused { reason: String, at_ms: u64 },
    Resumed { next_scheduled_at_ms: Option<u64>, at_ms: u64 },
    OccurrenceRecorded { occurrence: RoutineOccurrence, next_scheduled_at_ms: Option<u64>, at_ms: u64 },
    OccurrenceSkipped { occurrence: RoutineOccurrence, next_scheduled_at_ms: Option<u64>, at_ms: u64 },
    RunLinked { occurrence_id: ResourceId, run_id: ResourceId, at_ms: u64 },
    OccurrenceCompleted { occurrence_id: ResourceId, at_ms: u64 },
    OccurrenceFailed { occurrence_id: ResourceId, error: String, at_ms: u64 },
    Trashed { at_ms: u64 },
    Blocked { reason: String, at_ms: u64 },
}

impl RoutineEvent {
    pub fn at_ms(&self) -> u64 {
        match self {
            Self::Created { at_ms, .. }
            | Self::Updated { at_ms, .. }
            | Self::Paused { at_ms, .. }
            | Self::Resumed { at_ms, .. }
            | Self::OccurrenceRecorded { at_ms, .. }
            | Self::OccurrenceSkipped { at_ms, .. }
            | Self::RunLinked { at_ms, .. }
            | Self::OccurrenceCompleted { at_ms, .. }
            | Self::OccurrenceFailed { at_ms, .. }
            | Self::Trashed { at_ms }
            | Self::Blocked { at_ms, .. } => *at_ms,
        }
    }
}
