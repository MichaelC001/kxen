//! Durable locator for aggregates that need explicit recovery. Aggregate
//! streams remain authoritative; this registry only makes blocked evidence
//! discoverable across product domains and process restarts.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::core::event_store::{EventEntry, EventStore};
use crate::core::identity::{ActorRef, AggregateKind, AggregateRef, IdempotencyKey, ResourceId, SchemaVersion, Sequence, TraceContext};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryRecord {
    pub recovery_id: ResourceId,
    pub aggregate: AggregateRef,
    pub reason: String,
    pub evidence: Vec<String>,
    pub opened_at_ms: u64,
    pub resolved_at_ms: Option<u64>,
    pub resolution: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum RecoveryEvent {
    Opened { record: RecoveryRecord },
    Resolved { aggregate: AggregateRef, resolution: String, at_ms: u64 },
}

pub struct RecoveryRegistry {
    root: PathBuf,
    lock: std::sync::Mutex<()>,
}

impl RecoveryRegistry {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into(), lock: std::sync::Mutex::new(()) }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn open(
        &self,
        aggregate: AggregateRef,
        reason: impl Into<String>,
        evidence: Vec<String>,
        at_ms: u64,
    ) -> Result<RecoveryRecord, RecoveryError> {
        let reason = reason.into();
        if reason.trim().is_empty() {
            return Err(RecoveryError::Invalid("recovery reason is empty".into()));
        }
        let _guard = crate::core::shared::lock(&self.lock);
        let (records, seq) = self.load()?;
        if let Some(existing) = records.get(&aggregate).filter(|record| record.resolved_at_ms.is_none()) {
            return if existing.reason == reason {
                Ok(existing.clone())
            } else {
                Err(RecoveryError::Invalid(format!("aggregate already has open recovery: {}", existing.recovery_id)))
            };
        }
        let predecessor = records.get(&aggregate).map_or("initial", |record| record.recovery_id.as_str());
        let recovery_id = deterministic("recovery", &[aggregate.id.as_str(), &format!("{:?}", aggregate.kind), &reason, predecessor])?;
        let record = RecoveryRecord {
            recovery_id,
            aggregate: aggregate.clone(),
            reason,
            evidence,
            opened_at_ms: at_ms,
            resolved_at_ms: None,
            resolution: None,
        };
        self.append(
            seq,
            deterministic_key("recovery_open", &[record.recovery_id.as_str()])?,
            RecoveryEvent::Opened { record: record.clone() },
        )?;
        Ok(record)
    }

    pub fn resolve(
        &self,
        aggregate: &AggregateRef,
        resolution: impl Into<String>,
        at_ms: u64,
    ) -> Result<Option<RecoveryRecord>, RecoveryError> {
        let resolution = resolution.into();
        if resolution.trim().is_empty() {
            return Err(RecoveryError::Invalid("recovery resolution is empty".into()));
        }
        let _guard = crate::core::shared::lock(&self.lock);
        let (records, seq) = self.load()?;
        let Some(record) = records.get(aggregate).filter(|record| record.resolved_at_ms.is_none()) else { return Ok(None) };
        self.append(
            seq,
            deterministic_key("recovery_resolve", &[record.recovery_id.as_str()])?,
            RecoveryEvent::Resolved { aggregate: aggregate.clone(), resolution, at_ms },
        )?;
        Ok(self.load()?.0.get(aggregate).cloned())
    }

    pub fn list_open(&self) -> Result<Vec<RecoveryRecord>, RecoveryError> {
        let mut records = self.load()?.0.into_values().filter(|record| record.resolved_at_ms.is_none()).collect::<Vec<_>>();
        records.sort_by(|left, right| left.opened_at_ms.cmp(&right.opened_at_ms).then_with(|| left.recovery_id.cmp(&right.recovery_id)));
        Ok(records)
    }

    fn append(&self, seq: Sequence, key: IdempotencyKey, event: RecoveryEvent) -> Result<(), RecoveryError> {
        let event_id = deterministic("recevt", &[key.as_str()])?;
        self.store().append(
            seq,
            key,
            ActorRef::System { actor: crate::core::identity::SystemActor::Recovery },
            TraceContext::default(),
            vec![EventEntry { event_id, payload: event }],
        )?;
        Ok(())
    }

    fn load(&self) -> Result<(BTreeMap<AggregateRef, RecoveryRecord>, Sequence), RecoveryError> {
        let mut records = BTreeMap::new();
        let mut seq = Sequence(0);
        for batch in self.store().load()? {
            seq = batch.last_seq()?;
            for entry in batch.events {
                match entry.payload {
                    RecoveryEvent::Opened { record } => {
                        records.insert(record.aggregate.clone(), record);
                    }
                    RecoveryEvent::Resolved { aggregate, resolution, at_ms } => {
                        let record =
                            records.get_mut(&aggregate).ok_or_else(|| RecoveryError::Invalid("resolved recovery is missing".into()))?;
                        record.resolved_at_ms = Some(at_ms);
                        record.resolution = Some(resolution);
                    }
                }
            }
        }
        Ok((records, seq))
    }

    fn store(&self) -> EventStore<RecoveryEvent> {
        let id = ResourceId::parse("recovery_registry").expect("static recovery registry id");
        EventStore::new(
            self.root.join("registry"),
            AggregateRef { kind: AggregateKind::Recovery, id },
            SchemaVersion::new(1).expect("positive schema version"),
        )
    }
}

fn deterministic(prefix: &str, parts: &[&str]) -> Result<ResourceId, RecoveryError> {
    let mut bytes = Vec::new();
    for part in parts {
        bytes.extend_from_slice(&(part.len() as u64).to_be_bytes());
        bytes.extend_from_slice(part.as_bytes());
    }
    let hash = crate::core::identity::ContentHash::from_bytes(&bytes);
    ResourceId::parse(format!("{prefix}_{}", &hash.as_str()["sha256:".len()..])).map_err(RecoveryError::Invalid)
}

fn deterministic_key(prefix: &str, parts: &[&str]) -> Result<IdempotencyKey, RecoveryError> {
    deterministic(prefix, parts).and_then(|id| IdempotencyKey::parse(id.to_string()).map_err(RecoveryError::Invalid))
}

#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    #[error(transparent)]
    EventStore(#[from] crate::core::event_store::EventStoreError),
    #[error("recovery registry invalid: {0}")]
    Invalid(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_is_idempotent_and_resolution_survives_restart() {
        let root = std::env::temp_dir().join(format!("kxen-recovery-{}", uuid::Uuid::new_v4()));
        let aggregate = AggregateRef { kind: AggregateKind::BotRun, id: ResourceId::parse("brun_blocked").unwrap() };
        let registry = RecoveryRegistry::new(&root);
        let first = registry.open(aggregate.clone(), "UNKNOWN tool outcome", vec!["op_one".into()], 1).unwrap();
        assert_eq!(registry.open(aggregate.clone(), "UNKNOWN tool outcome", vec![], 2).unwrap(), first);
        assert_eq!(RecoveryRegistry::new(&root).list_open().unwrap(), std::slice::from_ref(&first));
        let resolved = registry.resolve(&aggregate, "owner reconciled evidence", 3).unwrap().unwrap();
        assert_eq!(resolved.resolution.as_deref(), Some("owner reconciled evidence"));
        assert!(RecoveryRegistry::new(&root).list_open().unwrap().is_empty());
        let reopened = registry.open(aggregate.clone(), "UNKNOWN tool outcome", vec!["op_two".into()], 4).unwrap();
        assert_ne!(reopened.recovery_id, first.recovery_id);
        assert_eq!(RecoveryRegistry::new(&root).list_open().unwrap(), vec![reopened]);
        std::fs::remove_dir_all(root).ok();
    }
}
