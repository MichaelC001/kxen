//! Bot-owned structured Memory. Conversation transcript never writes this store implicitly.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::core::event_store::{EventEntry, EventStore};
use crate::core::identity::{ActorRef, AggregateKind, AggregateRef, IdempotencyKey, ResourceId, SchemaVersion, Sequence, TraceContext};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Fact,
    Preference,
    Procedure,
    Constraint,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryItem {
    pub item_id: ResourceId,
    pub kind: MemoryKind,
    pub content: String,
    pub provenance: AggregateRef,
    pub version: u64,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryState {
    pub event_version: u64,
    pub items: BTreeMap<ResourceId, MemoryItem>,
}

#[derive(Clone, Debug)]
pub enum MemoryCommand {
    Create { item: MemoryItem },
    Revise { item_id: ResourceId, expected_item_version: u64, content: String, at_ms: u64 },
    Remove { item_id: ResourceId, expected_item_version: u64, at_ms: u64 },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum MemoryEvent {
    Created { item: MemoryItem },
    Revised { item_id: ResourceId, version: u64, content: String, at_ms: u64 },
    Removed { item_id: ResourceId, version: u64, at_ms: u64 },
}

pub struct MemoryRepository {
    root: PathBuf,
}

impl MemoryRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn get(&self, bot_id: &ResourceId) -> Result<MemoryState, MemoryError> {
        self.load(bot_id).map(|loaded| loaded.state)
    }

    pub fn execute(&self, request: MemoryWrite) -> Result<MemoryState, MemoryError> {
        authorize(&request.actor, &request.bot_id)?;
        let mut loaded = self.load(&request.bot_id)?;
        let duplicate_index = loaded.batches.iter().position(|batch| batch.idempotency_key == request.idempotency_key);
        if let Some(index) = duplicate_index {
            loaded.state = replay(&loaded.batches[..index])?.0;
        } else if loaded.last_seq.0 != request.expected_version {
            return Err(MemoryError::VersionConflict { expected: request.expected_version, actual: loaded.last_seq.0 });
        }
        let event = decide(&loaded.state, request.command)?;
        let mut projected = loaded.state.clone();
        apply(&mut projected, &event)?;
        let event_id = crate::bot::ids::deterministic_id("mevt", &[request.idempotency_key.as_str()]).map_err(MemoryError::Invalid)?;
        loaded.store.append(
            loaded.last_seq,
            request.idempotency_key,
            request.actor,
            request.trace,
            vec![EventEntry { event_id, payload: event }],
        )?;
        self.get(&request.bot_id)
    }

    fn load(&self, bot_id: &ResourceId) -> Result<LoadedMemory, MemoryError> {
        let store = EventStore::new(
            self.root.join("memory").join(bot_id.as_str()),
            AggregateRef { kind: AggregateKind::BotMemory, id: bot_id.clone() },
            SchemaVersion::new(1).expect("positive schema version"),
        );
        let batches = store.load()?;
        let (state, last_seq) = replay(&batches)?;
        Ok(LoadedMemory { store, state, last_seq, batches })
    }
}

struct LoadedMemory {
    store: EventStore<MemoryEvent>,
    state: MemoryState,
    last_seq: Sequence,
    batches: Vec<crate::core::event_store::EventBatch<MemoryEvent>>,
}

fn replay(batches: &[crate::core::event_store::EventBatch<MemoryEvent>]) -> Result<(MemoryState, Sequence), MemoryError> {
    let mut state = MemoryState::default();
    let mut last_seq = Sequence(0);
    for batch in batches {
        last_seq = batch.last_seq()?;
        for event in &batch.events {
            apply(&mut state, &event.payload)?;
        }
    }
    Ok((state, last_seq))
}

pub struct MemoryWrite {
    pub bot_id: ResourceId,
    pub expected_version: u64,
    pub idempotency_key: IdempotencyKey,
    pub actor: ActorRef,
    pub trace: TraceContext,
    pub command: MemoryCommand,
}

fn decide(state: &MemoryState, command: MemoryCommand) -> Result<MemoryEvent, MemoryError> {
    match command {
        MemoryCommand::Create { item } => {
            validate_content(&item.content)?;
            if item.version != 1 || state.items.contains_key(&item.item_id) {
                return Err(MemoryError::Rejected("Memory item version or id is invalid".into()));
            }
            Ok(MemoryEvent::Created { item })
        }
        MemoryCommand::Revise { item_id, expected_item_version, content, at_ms } => {
            validate_content(&content)?;
            let item = state.items.get(&item_id).ok_or_else(|| MemoryError::NotFound(item_id.to_string()))?;
            if item.version != expected_item_version {
                return Err(MemoryError::VersionConflict { expected: expected_item_version, actual: item.version });
            }
            let version = item.version.checked_add(1).ok_or_else(|| MemoryError::Rejected("Memory version overflow".into()))?;
            Ok(MemoryEvent::Revised { item_id, version, content, at_ms })
        }
        MemoryCommand::Remove { item_id, expected_item_version, at_ms } => {
            let item = state.items.get(&item_id).ok_or_else(|| MemoryError::NotFound(item_id.to_string()))?;
            if item.version != expected_item_version {
                return Err(MemoryError::VersionConflict { expected: expected_item_version, actual: item.version });
            }
            Ok(MemoryEvent::Removed { item_id, version: item.version, at_ms })
        }
    }
}

fn apply(state: &mut MemoryState, event: &MemoryEvent) -> Result<(), MemoryError> {
    match event {
        MemoryEvent::Created { item } => {
            validate_content(&item.content)?;
            if state.items.insert(item.item_id.clone(), item.clone()).is_some() {
                return Err(MemoryError::InvalidEvent("duplicate Memory item".into()));
            }
        }
        MemoryEvent::Revised { item_id, version, content, at_ms } => {
            validate_content(content)?;
            let item = state.items.get_mut(item_id).ok_or_else(|| MemoryError::InvalidEvent("Memory item missing".into()))?;
            if *version != item.version + 1 {
                return Err(MemoryError::InvalidEvent("Memory item version gap".into()));
            }
            item.version = *version;
            item.content = content.clone();
            item.updated_at_ms = *at_ms;
        }
        MemoryEvent::Removed { item_id, version, at_ms: _ } => {
            let item = state.items.get(item_id).ok_or_else(|| MemoryError::InvalidEvent("Memory item missing".into()))?;
            if &item.version != version {
                return Err(MemoryError::InvalidEvent("Memory remove version mismatch".into()));
            }
            state.items.remove(item_id);
        }
    }
    state.event_version = state.event_version.checked_add(1).ok_or_else(|| MemoryError::InvalidEvent("event version overflow".into()))?;
    Ok(())
}

fn authorize(actor: &ActorRef, bot_id: &ResourceId) -> Result<(), MemoryError> {
    if actor == &ActorRef::Owner || actor == &(ActorRef::Bot { id: bot_id.clone() }) {
        Ok(())
    } else {
        Err(MemoryError::Rejected("Memory actor does not own this Bot".into()))
    }
}

fn validate_content(content: &str) -> Result<(), MemoryError> {
    let normalized = content.to_ascii_lowercase();
    let secret_markers = ["password", "api_key", "api-key", "bearer ", "private key", "cookie:", "secret="];
    if content.trim().is_empty() || secret_markers.iter().any(|marker| normalized.contains(marker)) {
        Err(MemoryError::Rejected("Memory cannot contain empty content, secrets or credentials".into()))
    } else {
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error(transparent)]
    EventStore(#[from] crate::core::event_store::EventStoreError),
    #[error("Memory not found: {0}")]
    NotFound(String),
    #[error("Memory version conflict: expected {expected}, actual {actual}")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("Memory command rejected: {0}")]
    Rejected(String),
    #[error("Memory event invalid: {0}")]
    InvalidEvent(String),
    #[error("Memory id invalid: {0}")]
    Invalid(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_is_versioned_owned_and_rejects_secrets() {
        let root = std::env::temp_dir().join(format!("kxen-memory-{}", uuid::Uuid::new_v4()));
        let repo = MemoryRepository::new(&root);
        let bot_id = ResourceId::parse("bot_memory").unwrap();
        let item_id = ResourceId::parse("memory_one").unwrap();
        let write = |expected, key: &str, command| MemoryWrite {
            bot_id: bot_id.clone(),
            expected_version: expected,
            idempotency_key: IdempotencyKey::parse(key).unwrap(),
            actor: ActorRef::Bot { id: bot_id.clone() },
            trace: TraceContext::default(),
            command,
        };
        let item = MemoryItem {
            item_id: item_id.clone(),
            kind: MemoryKind::Procedure,
            content: "Always verify the report totals".into(),
            provenance: AggregateRef { kind: AggregateKind::BotRun, id: ResourceId::parse("brun_one").unwrap() },
            version: 1,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        let created = repo.execute(write(0, "idem_create", MemoryCommand::Create { item: item.clone() })).unwrap();
        let revised = repo
            .execute(write(
                created.event_version,
                "idem_revise",
                MemoryCommand::Revise {
                    item_id: item_id.clone(),
                    expected_item_version: 1,
                    content: "Verify totals and citations".into(),
                    at_ms: 2,
                },
            ))
            .unwrap();
        assert_eq!(revised.items[&item_id].version, 2);
        let secret = repo.execute(write(
            revised.event_version,
            "idem_secret",
            MemoryCommand::Revise { item_id: item_id.clone(), expected_item_version: 2, content: "api_key=abc".into(), at_ms: 3 },
        ));
        assert!(matches!(secret, Err(MemoryError::Rejected(_))));
        let retried = repo.execute(write(0, "idem_create", MemoryCommand::Create { item })).unwrap();
        assert_eq!(retried, revised);
        assert_eq!(std::fs::read_to_string(root.join("memory/bot_memory/events.jsonl")).unwrap().lines().count(), 2);
        std::fs::remove_dir_all(root).ok();
    }
}
