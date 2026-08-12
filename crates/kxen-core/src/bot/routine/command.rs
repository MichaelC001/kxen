use crate::core::identity::ResourceId;

use super::types::RoutineDefinition;

#[derive(Clone, Debug)]
pub enum RoutineCommand {
    Create { routine_id: ResourceId, definition: RoutineDefinition, at_ms: u64 },
    Update { definition: RoutineDefinition, at_ms: u64 },
    Tick { observed_at_ms: u64, resolved_revision_id: Option<ResourceId> },
    RunNow { occurrence_id: ResourceId, resolved_revision_id: ResourceId, at_ms: u64 },
    LinkRun { occurrence_id: ResourceId, run_id: ResourceId, at_ms: u64 },
    RecordResult { occurrence_id: ResourceId, error: Option<String>, at_ms: u64 },
    Pause { reason: String, at_ms: u64 },
    Resume { at_ms: u64 },
    Trash { at_ms: u64 },
    Block { reason: String, at_ms: u64 },
}
