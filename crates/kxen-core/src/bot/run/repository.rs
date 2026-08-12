use std::path::{Path, PathBuf};

use crate::core::event_store::{EventEntry, EventStore};
use crate::core::identity::{ActorRef, AggregateKind, AggregateRef, IdempotencyKey, ResourceId, SchemaVersion, Sequence, TraceContext};

use super::RunError;
use super::command::RunCommand;
use super::decision;
use super::events::RunEvent;
use super::projection;
use super::types::BotRunState;

const RUN_SCHEMA_VERSION: u32 = 1;

pub struct RunRepository {
    root: PathBuf,
}

impl RunRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn execute(&self, request: RunWrite) -> Result<BotRunState, RunError> {
        let loaded = self.load(&request.run_id)?;
        let duplicate_index = loaded.batches.iter().position(|batch| batch.idempotency_key == request.idempotency_key);
        let decision_state = if let Some(index) = duplicate_index { replay(&loaded.batches[..index])?.0 } else { loaded.state.clone() };
        if duplicate_index.is_none() && loaded.last_seq.0 != request.expected_version {
            return Err(RunError::VersionConflict { expected: request.expected_version, actual: loaded.last_seq.0 });
        }
        let events = decision::decide(decision_state.as_ref(), request.command)?;
        if events.is_empty() {
            return loaded.state.ok_or_else(|| RunError::NotFound(request.run_id.to_string()));
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
                let event_id =
                    crate::bot::ids::deterministic_id("revt", &[request.idempotency_key.as_str(), &index]).map_err(RunError::InvalidId)?;
                Ok(EventEntry { event_id, payload })
            })
            .collect::<Result<Vec<_>, RunError>>()?;
        loaded.store.append(loaded.last_seq, request.idempotency_key, request.actor, request.trace, entries)?;
        self.get(&request.run_id)
    }

    pub fn get(&self, run_id: &ResourceId) -> Result<BotRunState, RunError> {
        self.load(run_id)?.state.ok_or_else(|| RunError::NotFound(run_id.to_string()))
    }

    pub fn list(&self) -> Result<Vec<BotRunState>, RunError> {
        let root = self.root.join("runs");
        let mut runs = Vec::new();
        match std::fs::read_dir(root) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry.map_err(crate::core::event_store::EventStoreError::from)?;
                    if !entry.file_type().map_err(crate::core::event_store::EventStoreError::from)?.is_dir() {
                        continue;
                    }
                    let id = ResourceId::parse(entry.file_name().to_string_lossy()).map_err(RunError::InvalidId)?;
                    runs.push(self.get(&id)?);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(crate::core::event_store::EventStoreError::Io(error).into()),
        }
        runs.sort_by(|left, right| right.created_at_ms.cmp(&left.created_at_ms).then_with(|| left.spec.run_id.cmp(&right.spec.run_id)));
        Ok(runs)
    }

    pub fn recoverable(&self) -> Result<Vec<BotRunState>, RunError> {
        Ok(self
            .list()?
            .into_iter()
            .filter(|run| matches!(run.status, super::types::RunStatus::Queued | super::types::RunStatus::Running))
            .collect())
    }

    fn load(&self, run_id: &ResourceId) -> Result<LoadedRun, RunError> {
        let store = self.store(run_id);
        let batches = store.load()?;
        let (state, last_seq) = replay(&batches)?;
        Ok(LoadedRun { store, state, last_seq, batches })
    }

    fn store(&self, run_id: &ResourceId) -> EventStore<RunEvent> {
        EventStore::new(
            self.root.join("runs").join(run_id.as_str()),
            AggregateRef { kind: AggregateKind::BotRun, id: run_id.clone() },
            SchemaVersion::new(RUN_SCHEMA_VERSION).expect("positive schema version"),
        )
    }
}

struct LoadedRun {
    store: EventStore<RunEvent>,
    state: Option<BotRunState>,
    last_seq: Sequence,
    batches: Vec<crate::core::event_store::EventBatch<RunEvent>>,
}

fn replay(batches: &[crate::core::event_store::EventBatch<RunEvent>]) -> Result<(Option<BotRunState>, Sequence), RunError> {
    let mut state = None;
    let mut last_seq = Sequence(0);
    for batch in batches {
        last_seq = batch.last_seq()?;
        for event in &batch.events {
            projection::apply(&mut state, &event.payload)?;
        }
    }
    Ok((state, last_seq))
}

pub struct RunWrite {
    pub run_id: ResourceId,
    pub expected_version: u64,
    pub idempotency_key: IdempotencyKey,
    pub actor: ActorRef,
    pub trace: TraceContext,
    pub command: RunCommand,
}
