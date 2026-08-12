use serde::{Deserialize, Serialize};

use crate::core::durability::CommitError;
use crate::core::identity::{ActorRef, AggregateRef, ContentHash, IdempotencyKey, ResourceId, SchemaVersion, Sequence, TraceContext};
use crate::core::journal::JournalError;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventEntry<E> {
    pub event_id: ResourceId,
    pub payload: E,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EventBatch<E> {
    pub schema_version: SchemaVersion,
    pub batch_id: ResourceId,
    pub aggregate: AggregateRef,
    pub first_seq: Sequence,
    pub recorded_at_ms: u64,
    pub actor: ActorRef,
    pub trace: TraceContext,
    pub idempotency_key: IdempotencyKey,
    pub command_hash: ContentHash,
    pub events: Vec<EventEntry<E>>,
    pub checksum: ContentHash,
}

impl<E> EventBatch<E> {
    pub fn last_seq(&self) -> Result<Sequence, EventStoreError> {
        let offset = u64::try_from(self.events.len().saturating_sub(1)).map_err(|_| EventStoreError::SequenceOverflow)?;
        self.first_seq.0.checked_add(offset).map(Sequence).ok_or(EventStoreError::SequenceOverflow)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppendReceipt {
    pub batch_id: ResourceId,
    pub first_seq: Sequence,
    pub last_seq: Sequence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppendOutcome {
    Committed(AppendReceipt),
    Duplicate(AppendReceipt),
}

pub trait Projector<E> {
    type State;
    type Error: std::fmt::Display;

    fn apply(state: &mut Self::State, event: &E) -> Result<(), Self::Error>;
}

#[derive(Debug, thiserror::Error)]
pub enum EventStoreError {
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[error(transparent)]
    Commit(#[from] CommitError),
    #[error("event store IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("event store locked by another process")]
    Locked,
    #[error("invalid event batch: {0}")]
    InvalidBatch(String),
    #[error("event store version conflict: expected {expected}, actual {actual}")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("event store sequence gap: expected {expected}, actual {actual}")]
    SequenceGap { expected: u64, actual: u64 },
    #[error("event store id collision: {0}")]
    IdCollision(String),
    #[error("event store aggregate mismatch")]
    AggregateMismatch,
    #[error("unsupported event schema version {0}")]
    UnsupportedSchema(u32),
    #[error("event batch checksum mismatch: {0}")]
    ChecksumMismatch(String),
    #[error("event sequence overflow")]
    SequenceOverflow,
    #[error("event codec: {0}")]
    Codec(String),
    #[error("event projection: {0}")]
    Projection(String),
}

#[derive(Serialize)]
pub(super) struct CommandBody<'a, E> {
    pub aggregate: &'a AggregateRef,
    pub actor: &'a ActorRef,
    pub trace: &'a TraceContext,
    pub events: &'a [EventEntry<E>],
}

#[derive(Serialize)]
pub(super) struct BatchBody<'a, E> {
    pub schema_version: SchemaVersion,
    pub batch_id: &'a ResourceId,
    pub aggregate: &'a AggregateRef,
    pub first_seq: Sequence,
    pub recorded_at_ms: u64,
    pub actor: &'a ActorRef,
    pub trace: &'a TraceContext,
    pub idempotency_key: &'a IdempotencyKey,
    pub command_hash: &'a ContentHash,
    pub events: &'a [EventEntry<E>],
}

impl<'a, E> From<&'a EventBatch<E>> for BatchBody<'a, E> {
    fn from(batch: &'a EventBatch<E>) -> Self {
        Self {
            schema_version: batch.schema_version,
            batch_id: &batch.batch_id,
            aggregate: &batch.aggregate,
            first_seq: batch.first_seq,
            recorded_at_ms: batch.recorded_at_ms,
            actor: &batch.actor,
            trace: &batch.trace,
            idempotency_key: &batch.idempotency_key,
            command_hash: &batch.command_hash,
            events: &batch.events,
        }
    }
}
