//! Typed append-only aggregate event batches.

mod lock;
mod types;

pub use types::{AppendOutcome, AppendReceipt, EventBatch, EventEntry, EventStoreError, Projector};

use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::core::identity::{ActorRef, AggregateRef, ContentHash, IdempotencyKey, ResourceId, SchemaVersion, Sequence, TraceContext};
use crate::core::journal::{JournalCursor, StrictJsonl};

use types::{BatchBody, CommandBody};

const MAX_EVENTS_PER_BATCH: usize = 256;

pub struct EventStore<E> {
    root: PathBuf,
    aggregate: AggregateRef,
    schema_version: SchemaVersion,
    marker: std::marker::PhantomData<fn() -> E>,
}

impl<E> EventStore<E> {
    pub fn new(root: impl Into<PathBuf>, aggregate: AggregateRef, schema_version: SchemaVersion) -> Self {
        Self { root: root.into(), aggregate, schema_version, marker: std::marker::PhantomData }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn events_path(&self) -> PathBuf {
        self.root.join("events.jsonl")
    }
}

impl<E: DeserializeOwned + Serialize> EventStore<E> {
    pub fn load(&self) -> Result<Vec<EventBatch<E>>, EventStoreError> {
        let batches = StrictJsonl::new(self.events_path()).scan(JournalCursor::default())?.records;
        self.validate(&batches)?;
        Ok(batches)
    }

    pub fn append(
        &self,
        expected_last_seq: Sequence,
        idempotency_key: IdempotencyKey,
        actor: ActorRef,
        trace: TraceContext,
        events: Vec<EventEntry<E>>,
    ) -> Result<AppendOutcome, EventStoreError> {
        if events.is_empty() || events.len() > MAX_EVENTS_PER_BATCH {
            return Err(EventStoreError::InvalidBatch(format!("event count must be between 1 and {MAX_EVENTS_PER_BATCH}")));
        }
        unique_event_ids(&events)?;
        let _lock = lock::acquire(&self.root)?;
        let batches = self.load()?;
        let command_hash = hash(&CommandBody { aggregate: &self.aggregate, actor: &actor, trace: &trace, events: &events })?;
        if let Some(existing) = batches.iter().find(|batch| batch.idempotency_key == idempotency_key) {
            let same_request = existing.command_hash == command_hash
                || semantic_command_hash(&existing.aggregate, &existing.actor, &existing.trace, &existing.events)?
                    == semantic_command_hash(&self.aggregate, &actor, &trace, &events)?;
            if !same_request {
                return Err(EventStoreError::IdCollision(idempotency_key.as_str().to_string()));
            }
            return Ok(AppendOutcome::Duplicate(receipt(existing)?));
        }
        let actual_last_seq = batches.last().map(EventBatch::last_seq).transpose()?.unwrap_or(Sequence(0));
        if actual_last_seq != expected_last_seq {
            return Err(EventStoreError::VersionConflict { expected: expected_last_seq.0, actual: actual_last_seq.0 });
        }
        let existing_ids: HashSet<&str> =
            batches.iter().flat_map(|batch| batch.events.iter()).map(|entry| entry.event_id.as_str()).collect();
        if let Some(collision) = events.iter().find(|entry| existing_ids.contains(entry.event_id.as_str())) {
            return Err(EventStoreError::IdCollision(collision.event_id.to_string()));
        }
        let first_seq = Sequence(actual_last_seq.0.checked_add(1).ok_or(EventStoreError::SequenceOverflow)?);
        let mut batch = EventBatch {
            schema_version: self.schema_version,
            batch_id: ResourceId::new("ebatch").map_err(EventStoreError::InvalidBatch)?,
            aggregate: self.aggregate.clone(),
            first_seq,
            recorded_at_ms: crate::core::shared::now_ms(),
            actor,
            trace,
            idempotency_key,
            command_hash,
            events,
            checksum: ContentHash::from_bytes(b"pending"),
        };
        batch.checksum = batch_checksum(&batch)?;
        let receipt = receipt(&batch)?;
        StrictJsonl::new(self.events_path()).append(&batch, batches.len() as u64)?;
        Ok(AppendOutcome::Committed(receipt))
    }

    pub fn replay<P: Projector<E>>(&self, initial: P::State) -> Result<P::State, EventStoreError> {
        let mut state = initial;
        for batch in self.load()? {
            for entry in batch.events {
                P::apply(&mut state, &entry.payload).map_err(|error| EventStoreError::Projection(error.to_string()))?;
            }
        }
        Ok(state)
    }

    fn validate(&self, batches: &[EventBatch<E>]) -> Result<(), EventStoreError> {
        let mut next_seq = 1u64;
        let mut batch_ids = HashSet::new();
        let mut idempotency_keys = HashSet::new();
        let mut event_ids = HashSet::new();
        for batch in batches {
            if batch.schema_version != self.schema_version {
                return Err(EventStoreError::UnsupportedSchema(batch.schema_version.get()));
            }
            if batch.aggregate != self.aggregate {
                return Err(EventStoreError::AggregateMismatch);
            }
            if batch.events.is_empty() || batch.events.len() > MAX_EVENTS_PER_BATCH {
                return Err(EventStoreError::InvalidBatch("stored event count is outside limits".into()));
            }
            if batch.first_seq != Sequence(next_seq) {
                return Err(EventStoreError::SequenceGap { expected: next_seq, actual: batch.first_seq.0 });
            }
            if !batch_ids.insert(batch.batch_id.as_str()) || !idempotency_keys.insert(batch.idempotency_key.as_str()) {
                return Err(EventStoreError::IdCollision(batch.batch_id.to_string()));
            }
            for event in &batch.events {
                if !event_ids.insert(event.event_id.as_str()) {
                    return Err(EventStoreError::IdCollision(event.event_id.to_string()));
                }
            }
            let command_hash =
                hash(&CommandBody { aggregate: &batch.aggregate, actor: &batch.actor, trace: &batch.trace, events: &batch.events })?;
            if command_hash != batch.command_hash || batch_checksum(batch)? != batch.checksum {
                return Err(EventStoreError::ChecksumMismatch(batch.batch_id.to_string()));
            }
            next_seq = batch.last_seq()?.0.checked_add(1).ok_or(EventStoreError::SequenceOverflow)?;
        }
        Ok(())
    }
}

fn unique_event_ids<E>(events: &[EventEntry<E>]) -> Result<(), EventStoreError> {
    let mut ids = HashSet::new();
    for event in events {
        if !ids.insert(event.event_id.as_str()) {
            return Err(EventStoreError::IdCollision(event.event_id.to_string()));
        }
    }
    Ok(())
}

fn receipt<E>(batch: &EventBatch<E>) -> Result<AppendReceipt, EventStoreError> {
    Ok(AppendReceipt { batch_id: batch.batch_id.clone(), first_seq: batch.first_seq, last_seq: batch.last_seq()? })
}

fn batch_checksum<E: Serialize>(batch: &EventBatch<E>) -> Result<ContentHash, EventStoreError> {
    hash(&BatchBody::from(batch))
}

fn hash(value: &impl Serialize) -> Result<ContentHash, EventStoreError> {
    serde_json::to_vec(value).map(|bytes| ContentHash::from_bytes(&bytes)).map_err(|error| EventStoreError::Codec(error.to_string()))
}

fn semantic_command_hash<E: Serialize>(
    aggregate: &AggregateRef,
    actor: &ActorRef,
    trace: &TraceContext,
    events: &[EventEntry<E>],
) -> Result<ContentHash, EventStoreError> {
    let mut events = serde_json::to_value(events).map_err(|error| EventStoreError::Codec(error.to_string()))?;
    strip_server_times(&mut events);
    hash(&serde_json::json!({ "aggregate": aggregate, "actor": actor, "trace": trace, "events": events }))
}

fn strip_server_times(value: &mut serde_json::Value) {
    if let serde_json::Value::Array(values) = value {
        for entry in values {
            if let Some(payload) = entry.get_mut("payload") {
                strip_event_time(payload);
            }
        }
    }
}

fn strip_event_time(value: &mut serde_json::Value) {
    let serde_json::Value::Object(payload) = value else {
        strip_generated_times(value);
        return;
    };
    if payload.len() == 1
        && let Some(serde_json::Value::Object(body)) = payload.values_mut().next()
    {
        strip_event_body(body);
    } else {
        strip_event_body(payload);
    }
}

fn strip_event_body(body: &mut serde_json::Map<String, serde_json::Value>) {
    body.remove("at_ms");
    strip_generated_fields(body);
    for (field, value) in body {
        if field == "event" {
            strip_event_time(value);
        } else {
            strip_generated_times(value);
        }
    }
}

fn strip_generated_times(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => values.iter_mut().for_each(strip_generated_times),
        serde_json::Value::Object(object) => {
            strip_generated_fields(object);
            object.values_mut().for_each(strip_generated_times);
        }
        _ => {}
    }
}

fn strip_generated_fields(object: &mut serde_json::Map<String, serde_json::Value>) {
    for field in ["created_at_ms", "updated_at_ms", "granted_at_ms", "recorded_at_ms", "opened_at_ms", "resolved_at_ms"] {
        if object.get(field).is_some_and(serde_json::Value::is_number) {
            object.remove(field);
        }
    }
}

#[cfg(test)]
mod tests;
