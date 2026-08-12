use std::path::{Path, PathBuf};

use crate::core::event_store::{EventBatch, EventEntry, EventStore};
use crate::core::identity::{ActorRef, AggregateKind, AggregateRef, IdempotencyKey, ResourceId, SchemaVersion, Sequence, TraceContext};

use super::ConversationError;
use super::command::ConversationCommand;
use super::decision;
use super::events::ConversationEvent;
use super::projection;
use super::types::ConversationState;

const CONVERSATION_SCHEMA_VERSION: u32 = 1;

pub struct ConversationRepository {
    root: PathBuf,
}

impl ConversationRepository {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn execute(&self, request: ConversationWrite) -> Result<ConversationState, ConversationError> {
        let loaded = self.load(&request.conversation_id)?;
        let duplicate_index = loaded.batches.iter().position(|batch| batch.idempotency_key == request.idempotency_key);
        let decision_state = if let Some(index) = duplicate_index { replay(&loaded.batches[..index])?.0 } else { loaded.state.clone() };
        if duplicate_index.is_none() && loaded.last_seq.0 != request.expected_version {
            return Err(ConversationError::VersionConflict { expected: request.expected_version, actual: loaded.last_seq.0 });
        }
        let events = decision::decide(decision_state.as_ref(), &request.actor, request.command)?;
        if events.is_empty() {
            return loaded.state.ok_or_else(|| ConversationError::NotFound(request.conversation_id.to_string()));
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
                let event_id = crate::bot::ids::deterministic_id("cevt", &[request.idempotency_key.as_str(), &index])
                    .map_err(ConversationError::InvalidId)?;
                Ok(EventEntry { event_id, payload })
            })
            .collect::<Result<Vec<_>, ConversationError>>()?;
        loaded.store.append(loaded.last_seq, request.idempotency_key, request.actor, request.trace, entries)?;
        self.get(&request.conversation_id)
    }

    pub fn get(&self, conversation_id: &ResourceId) -> Result<ConversationState, ConversationError> {
        self.load(conversation_id)?.state.ok_or_else(|| ConversationError::NotFound(conversation_id.to_string()))
    }

    pub fn list(&self) -> Result<Vec<ConversationState>, ConversationError> {
        let mut conversations = Vec::new();
        match std::fs::read_dir(self.root.join("conversations")) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry.map_err(crate::core::event_store::EventStoreError::from)?;
                    if entry.file_type().map_err(crate::core::event_store::EventStoreError::from)?.is_dir() {
                        let id = ResourceId::parse(entry.file_name().to_string_lossy()).map_err(ConversationError::InvalidId)?;
                        conversations.push(self.get(&id)?);
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(crate::core::event_store::EventStoreError::Io(error).into()),
        }
        conversations.sort_by_key(|conversation| std::cmp::Reverse(conversation.updated_at_ms));
        Ok(conversations)
    }

    fn load(&self, conversation_id: &ResourceId) -> Result<LoadedConversation, ConversationError> {
        let store = self.store(conversation_id);
        let batches = store.load()?;
        let (state, last_seq) = replay(&batches)?;
        Ok(LoadedConversation { store, state, last_seq, batches })
    }

    fn store(&self, conversation_id: &ResourceId) -> EventStore<ConversationEvent> {
        EventStore::new(
            self.root.join("conversations").join(conversation_id.as_str()),
            AggregateRef { kind: AggregateKind::Conversation, id: conversation_id.clone() },
            SchemaVersion::new(CONVERSATION_SCHEMA_VERSION).expect("positive schema version"),
        )
    }
}

struct LoadedConversation {
    store: EventStore<ConversationEvent>,
    state: Option<ConversationState>,
    last_seq: Sequence,
    batches: Vec<EventBatch<ConversationEvent>>,
}

fn replay(batches: &[EventBatch<ConversationEvent>]) -> Result<(Option<ConversationState>, Sequence), ConversationError> {
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

pub struct ConversationWrite {
    pub conversation_id: ResourceId,
    pub expected_version: u64,
    pub idempotency_key: IdempotencyKey,
    pub actor: ActorRef,
    pub trace: TraceContext,
    pub command: ConversationCommand,
}
