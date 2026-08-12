use std::path::{Path, PathBuf};

use crate::core::event_store::{EventBatch, EventEntry, EventStore};
use crate::core::identity::{ActorRef, AggregateKind, AggregateRef, IdempotencyKey, ResourceId, SchemaVersion, Sequence, TraceContext};

use super::RoutineError;
use super::command::RoutineCommand;
use super::decision;
use super::events::RoutineEvent;
use super::projection;
use super::types::RoutineState;

const ROUTINE_SCHEMA_VERSION: u32 = 1;

pub struct RoutineRepository {
    root: PathBuf,
}

impl RoutineRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn execute(&self, request: RoutineWrite) -> Result<RoutineState, RoutineError> {
        let loaded = self.load(&request.routine_id)?;
        let duplicate_index = loaded.batches.iter().position(|batch| batch.idempotency_key == request.idempotency_key);
        let decision_state = if let Some(index) = duplicate_index { replay(&loaded.batches[..index])?.0 } else { loaded.state.clone() };
        if duplicate_index.is_none() && loaded.last_seq.0 != request.expected_version {
            return Err(RoutineError::VersionConflict { expected: request.expected_version, actual: loaded.last_seq.0 });
        }
        let events = decision::decide(decision_state.as_ref(), &request.actor, request.command)?;
        if events.is_empty() {
            return loaded.state.ok_or_else(|| RoutineError::NotFound(request.routine_id.to_string()));
        }
        let mut projected = decision_state;
        for event in &events {
            projection::apply(&mut projected, event)?;
        }
        let entries = events
            .into_iter()
            .enumerate()
            .map(|(index, payload)| {
                let index = index.to_string();
                let event_id = crate::bot::ids::deterministic_id("rtevt", &[request.idempotency_key.as_str(), &index])
                    .map_err(RoutineError::InvalidId)?;
                Ok(EventEntry { event_id, payload })
            })
            .collect::<Result<Vec<_>, RoutineError>>()?;
        loaded.store.append(loaded.last_seq, request.idempotency_key, request.actor, request.trace, entries)?;
        self.get(&request.routine_id)
    }

    pub fn get(&self, routine_id: &ResourceId) -> Result<RoutineState, RoutineError> {
        self.load(routine_id)?.state.ok_or_else(|| RoutineError::NotFound(routine_id.to_string()))
    }

    pub fn list(&self) -> Result<Vec<RoutineState>, RoutineError> {
        let mut routines = Vec::new();
        match std::fs::read_dir(self.root.join("routines")) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry.map_err(crate::core::event_store::EventStoreError::from)?;
                    if entry.file_type().map_err(crate::core::event_store::EventStoreError::from)?.is_dir() {
                        let id = ResourceId::parse(entry.file_name().to_string_lossy()).map_err(RoutineError::InvalidId)?;
                        routines.push(self.get(&id)?);
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(crate::core::event_store::EventStoreError::Io(error).into()),
        }
        routines.sort_by(|left, right| left.routine_id.cmp(&right.routine_id));
        Ok(routines)
    }

    fn load(&self, routine_id: &ResourceId) -> Result<LoadedRoutine, RoutineError> {
        let store = self.store(routine_id);
        let batches = store.load()?;
        let (state, last_seq) = replay(&batches)?;
        Ok(LoadedRoutine { store, state, last_seq, batches })
    }

    fn store(&self, routine_id: &ResourceId) -> EventStore<RoutineEvent> {
        EventStore::new(
            self.root.join("routines").join(routine_id.as_str()),
            AggregateRef { kind: AggregateKind::Routine, id: routine_id.clone() },
            SchemaVersion::new(ROUTINE_SCHEMA_VERSION).expect("positive schema version"),
        )
    }
}

struct LoadedRoutine {
    store: EventStore<RoutineEvent>,
    state: Option<RoutineState>,
    last_seq: Sequence,
    batches: Vec<EventBatch<RoutineEvent>>,
}

fn replay(batches: &[EventBatch<RoutineEvent>]) -> Result<(Option<RoutineState>, Sequence), RoutineError> {
    let mut state = None;
    let mut sequence = Sequence(0);
    for batch in batches {
        sequence = batch.last_seq()?;
        for event in &batch.events {
            projection::apply(&mut state, &event.payload)?;
        }
    }
    Ok((state, sequence))
}

pub struct RoutineWrite {
    pub routine_id: ResourceId,
    pub expected_version: u64,
    pub idempotency_key: IdempotencyKey,
    pub actor: ActorRef,
    pub trace: TraceContext,
    pub command: RoutineCommand,
}
