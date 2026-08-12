use super::*;
use crate::core::identity::{AggregateKind, SystemActor};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Event {
    Added { value: u64 },
    Removed { value: u64 },
    Timed { value: u64, at_ms: u64 },
}

fn store(name: &str) -> EventStore<Event> {
    let root = std::env::temp_dir().join(format!("kxen-event-store-{name}-{}", uuid::Uuid::new_v4()));
    EventStore::new(
        root,
        AggregateRef { kind: AggregateKind::BotRun, id: ResourceId::parse("brun_test").unwrap() },
        SchemaVersion::new(1).unwrap(),
    )
}

fn entry(id: &str, event: Event) -> EventEntry<Event> {
    EventEntry { event_id: ResourceId::parse(id).unwrap(), payload: event }
}

fn actor() -> ActorRef {
    ActorRef::System { actor: SystemActor::Runtime }
}

#[test]
fn appends_multiple_events_as_one_physical_record() {
    let store = store("batch");
    let outcome = store
        .append(
            Sequence(0),
            IdempotencyKey::parse("idem_batch").unwrap(),
            actor(),
            TraceContext::default(),
            vec![entry("evt_one", Event::Added { value: 1 }), entry("evt_two", Event::Added { value: 2 })],
        )
        .unwrap();
    assert!(matches!(outcome, AppendOutcome::Committed(AppendReceipt { first_seq: Sequence(1), last_seq: Sequence(2), .. })));
    assert_eq!(std::fs::read_to_string(store.events_path()).unwrap().lines().count(), 1);
    assert_eq!(store.load().unwrap()[0].events.len(), 2);
    std::fs::remove_dir_all(store.root()).ok();
}

#[test]
fn duplicate_is_noop_and_changed_command_is_collision() {
    let store = store("idempotency");
    let append = |value| {
        store.append(
            Sequence(0),
            IdempotencyKey::parse("idem_same").unwrap(),
            actor(),
            TraceContext::default(),
            vec![entry("evt_same", Event::Added { value })],
        )
    };
    assert!(matches!(append(1).unwrap(), AppendOutcome::Committed(_)));
    assert!(matches!(append(1).unwrap(), AppendOutcome::Duplicate(_)));
    assert!(matches!(append(2), Err(EventStoreError::IdCollision(_))));
    assert_eq!(std::fs::read_to_string(store.events_path()).unwrap().lines().count(), 1);
    std::fs::remove_dir_all(store.root()).ok();
}

#[test]
fn duplicate_ignores_new_server_time_but_not_business_fields() {
    let store = store("server-time-idempotency");
    let append = |value, at_ms| {
        store.append(
            Sequence(0),
            IdempotencyKey::parse("idem_timed").unwrap(),
            actor(),
            TraceContext::default(),
            vec![entry("evt_timed", Event::Timed { value, at_ms })],
        )
    };
    assert!(matches!(append(1, 10).unwrap(), AppendOutcome::Committed(_)));
    assert!(matches!(append(1, 20).unwrap(), AppendOutcome::Duplicate(_)));
    assert!(matches!(append(2, 20), Err(EventStoreError::IdCollision(_))));
    std::fs::remove_dir_all(store.root()).ok();
}

#[test]
fn expected_sequence_conflict_is_rejected() {
    let store = store("version");
    store
        .append(
            Sequence(0),
            IdempotencyKey::parse("idem_first").unwrap(),
            actor(),
            TraceContext::default(),
            vec![entry("evt_first", Event::Added { value: 1 })],
        )
        .unwrap();
    let error = store
        .append(
            Sequence(0),
            IdempotencyKey::parse("idem_second").unwrap(),
            actor(),
            TraceContext::default(),
            vec![entry("evt_second", Event::Added { value: 2 })],
        )
        .unwrap_err();
    assert!(matches!(error, EventStoreError::VersionConflict { expected: 0, actual: 1 }));
    std::fs::remove_dir_all(store.root()).ok();
}

#[test]
fn tampering_and_torn_tail_fail_closed() {
    let store = store("integrity");
    store
        .append(
            Sequence(0),
            IdempotencyKey::parse("idem_integrity").unwrap(),
            actor(),
            TraceContext::default(),
            vec![entry("evt_integrity", Event::Added { value: 1 })],
        )
        .unwrap();
    let path = store.events_path();
    let text = std::fs::read_to_string(&path).unwrap().replace("\"value\":1", "\"value\":2");
    std::fs::write(&path, text).unwrap();
    assert!(matches!(store.load(), Err(EventStoreError::ChecksumMismatch(_))));
    std::fs::write(&path, b"{\"partial\":").unwrap();
    assert!(matches!(store.load(), Err(EventStoreError::Journal(crate::core::journal::JournalError::Unterminated { .. }))));
    std::fs::remove_dir_all(store.root()).ok();
}

struct Sum;

impl Projector<Event> for Sum {
    type State = i64;
    type Error = String;

    fn apply(state: &mut Self::State, event: &Event) -> Result<(), Self::Error> {
        match event {
            Event::Added { value } => *state += *value as i64,
            Event::Removed { value } => *state -= *value as i64,
            Event::Timed { value, .. } => *state += *value as i64,
        }
        Ok(())
    }
}

#[test]
fn replay_is_deterministic() {
    let store = store("replay");
    store
        .append(
            Sequence(0),
            IdempotencyKey::parse("idem_replay").unwrap(),
            actor(),
            TraceContext::default(),
            vec![entry("evt_add", Event::Added { value: 5 }), entry("evt_remove", Event::Removed { value: 2 })],
        )
        .unwrap();
    assert_eq!(store.replay::<Sum>(0).unwrap(), 3);
    assert_eq!(store.replay::<Sum>(0).unwrap(), 3);
    std::fs::remove_dir_all(store.root()).ok();
}
