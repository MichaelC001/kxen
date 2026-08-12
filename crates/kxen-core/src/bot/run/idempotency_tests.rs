use super::tests::{id, repository, spec, write};
use super::*;
use crate::agent::dcp::ProviderNeutralPart;
use crate::core::identity::{ActorRef, IdempotencyKey, TraceContext};

#[test]
fn exact_write_retry_is_noop_and_changed_retry_collides() {
    let repo = repository("idempotency");
    let run_id = id("brun_idempotency");
    let command = || RunCommand::Queue { spec: Box::new(spec(run_id.clone())), at_ms: 10 };
    let first = write(&repo, &run_id, 0, "idem_queue", command());
    let running = write(&repo, &run_id, first.event_version, "idem_start", RunCommand::Start { at_ms: 20 });
    let duplicate = write(&repo, &run_id, 0, "idem_queue", command());
    assert_eq!(running, duplicate);
    let collision = repo.execute(RunWrite {
        run_id: run_id.clone(),
        expected_version: 0,
        idempotency_key: IdempotencyKey::parse("idem_queue").unwrap(),
        actor: ActorRef::Owner,
        trace: TraceContext::default(),
        command: RunCommand::Queue {
            spec: Box::new(RunSpec { input: vec![ProviderNeutralPart::Text { text: "changed".into() }], ..spec(run_id.clone()) }),
            at_ms: 11,
        },
    });
    assert!(matches!(collision, Err(RunError::EventStore(crate::core::event_store::EventStoreError::IdCollision(_)))));
    std::fs::remove_dir_all(repo.root()).ok();
}
