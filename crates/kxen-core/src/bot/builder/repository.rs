use std::path::{Path, PathBuf};

use crate::core::event_store::{EventBatch, EventEntry, EventStore};
use crate::core::identity::{ActorRef, AggregateKind, AggregateRef, IdempotencyKey, ResourceId, SchemaVersion, Sequence, TraceContext};

use super::BuilderError;
use super::command::BuilderCommand;
use super::decision;
use super::events::BuilderEvent;
use super::projection;
use super::types::BuilderState;

pub struct BuilderRepository {
    root: PathBuf,
}

impl BuilderRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn execute(&self, request: BuilderWrite) -> Result<BuilderState, BuilderError> {
        let loaded = self.load(&request.builder_session_id)?;
        let duplicate_index = loaded.batches.iter().position(|batch| batch.idempotency_key == request.idempotency_key);
        let decision_state = if let Some(index) = duplicate_index { replay(&loaded.batches[..index])?.0 } else { loaded.state.clone() };
        if duplicate_index.is_none() && loaded.last_seq.0 != request.expected_version {
            return Err(BuilderError::VersionConflict { expected: request.expected_version, actual: loaded.last_seq.0 });
        }
        let events = decision::decide(decision_state.as_ref(), &request.actor, request.command)?;
        let mut projected = decision_state;
        for event in &events {
            projection::apply(&mut projected, event)?;
        }
        let entries = events
            .into_iter()
            .enumerate()
            .map(|(index, payload)| {
                let index = index.to_string();
                let event_id = crate::bot::ids::deterministic_id("buevt", &[request.idempotency_key.as_str(), &index])
                    .map_err(BuilderError::InvalidId)?;
                Ok(EventEntry { event_id, payload })
            })
            .collect::<Result<Vec<_>, BuilderError>>()?;
        loaded.store.append(loaded.last_seq, request.idempotency_key, request.actor, request.trace, entries)?;
        self.get(&request.builder_session_id)
    }

    pub fn get(&self, builder_session_id: &ResourceId) -> Result<BuilderState, BuilderError> {
        self.load(builder_session_id)?.state.ok_or_else(|| BuilderError::NotFound(builder_session_id.to_string()))
    }

    pub fn list(&self) -> Result<Vec<BuilderState>, BuilderError> {
        let mut sessions = Vec::new();
        match std::fs::read_dir(self.root.join("definitions/builder-sessions")) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry.map_err(crate::core::event_store::EventStoreError::from)?;
                    if entry.file_type().map_err(crate::core::event_store::EventStoreError::from)?.is_dir() {
                        let id = ResourceId::parse(entry.file_name().to_string_lossy()).map_err(BuilderError::InvalidId)?;
                        sessions.push(self.get(&id)?);
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(crate::core::event_store::EventStoreError::Io(error).into()),
        }
        sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at_ms));
        Ok(sessions)
    }

    fn load(&self, builder_session_id: &ResourceId) -> Result<LoadedBuilder, BuilderError> {
        let store = EventStore::new(
            self.root.join("definitions/builder-sessions").join(builder_session_id.as_str()),
            AggregateRef { kind: AggregateKind::BuilderSession, id: builder_session_id.clone() },
            SchemaVersion::new(1).expect("positive schema version"),
        );
        let batches = store.load()?;
        let (state, last_seq) = replay(&batches)?;
        Ok(LoadedBuilder { store, state, last_seq, batches })
    }
}

struct LoadedBuilder {
    store: EventStore<BuilderEvent>,
    state: Option<BuilderState>,
    last_seq: Sequence,
    batches: Vec<EventBatch<BuilderEvent>>,
}

fn replay(batches: &[EventBatch<BuilderEvent>]) -> Result<(Option<BuilderState>, Sequence), BuilderError> {
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

pub struct BuilderWrite {
    pub builder_session_id: ResourceId,
    pub expected_version: u64,
    pub idempotency_key: IdempotencyKey,
    pub actor: ActorRef,
    pub trace: TraceContext,
    pub command: BuilderCommand,
}
