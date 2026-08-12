use super::*;
use crate::core::identity::{ActorRef, ResourceId};

fn id(value: &str) -> ResourceId {
    ResourceId::parse(value).unwrap()
}

fn envelope(delivery_id: &str, payload: &str) -> DeliveryEnvelope<String> {
    DeliveryEnvelope::new(id(delivery_id), ActorRef::Owner, 1, payload.into()).unwrap()
}

fn execute(state: &mut DeliveryProjection<String>, command: DeliveryCommand<String>) -> Result<DeliveryDecision<String>, DeliveryError> {
    let decision = state.decide(command)?;
    for event in decision.events.clone() {
        state.apply(event)?;
    }
    Ok(decision)
}

#[test]
fn enqueue_is_idempotent_by_content_and_rejects_collision() {
    let mut state = DeliveryProjection::new(8);
    let command = || DeliveryCommand::Enqueue { envelope: envelope("delivery_one", "payload"), position: QueuePosition::Back };
    assert!(!execute(&mut state, command()).unwrap().duplicate);
    assert!(execute(&mut state, command()).unwrap().duplicate);
    let changed = DeliveryCommand::Enqueue { envelope: envelope("delivery_one", "changed"), position: QueuePosition::Back };
    assert!(matches!(execute(&mut state, changed), Err(DeliveryError::IdCollision(_))));
    assert_eq!(state.queued, [id("delivery_one")]);
}

#[test]
fn claim_replays_until_matching_generation_acknowledges() {
    let mut state = DeliveryProjection::new(8);
    execute(&mut state, DeliveryCommand::Enqueue { envelope: envelope("delivery_one", "payload"), position: QueuePosition::Back }).unwrap();
    let claimed = execute(&mut state, DeliveryCommand::Claim { mode: ClaimMode::One, generation: id("claim_one") }).unwrap().claim.unwrap();
    let replay = state.decide(DeliveryCommand::Claim { mode: ClaimMode::One, generation: id("claim_other") }).unwrap().claim.unwrap();
    assert_eq!(replay, claimed);
    let stale = ClaimToken { generation: id("claim_stale"), delivery_ids: claimed.delivery_ids.clone() };
    assert!(matches!(execute(&mut state, DeliveryCommand::Acknowledge { token: stale }), Err(DeliveryError::StaleClaim)));
    execute(&mut state, DeliveryCommand::Acknowledge { token: claimed }).unwrap();
    assert!(state.records.is_empty());
    assert_eq!(state.tombstones[0].status, DeliveryStatus::Acked);
    let duplicate = DeliveryCommand::Enqueue { envelope: envelope("delivery_one", "payload"), position: QueuePosition::Back };
    assert!(execute(&mut state, duplicate).unwrap().duplicate);
}

#[test]
fn batch_release_preserves_fifo_order() {
    let mut state = DeliveryProjection::new(8);
    for name in ["delivery_one", "delivery_two", "delivery_three"] {
        execute(&mut state, DeliveryCommand::Enqueue { envelope: envelope(name, name), position: QueuePosition::Back }).unwrap();
    }
    let claim = execute(&mut state, DeliveryCommand::Claim { mode: ClaimMode::Batch { limit: 2 }, generation: id("claim_batch") })
        .unwrap()
        .claim
        .unwrap();
    execute(&mut state, DeliveryCommand::Release { token: claim }).unwrap();
    assert_eq!(state.queued, [id("delivery_one"), id("delivery_two"), id("delivery_three")]);
}

#[test]
fn reject_requires_claim_generation_and_block_removes_from_queue() {
    let mut state = DeliveryProjection::new(8);
    for name in ["delivery_one", "delivery_two"] {
        execute(&mut state, DeliveryCommand::Enqueue { envelope: envelope(name, name), position: QueuePosition::Back }).unwrap();
    }
    let claim = execute(&mut state, DeliveryCommand::Claim { mode: ClaimMode::One, generation: id("claim_one") }).unwrap().claim.unwrap();
    assert!(matches!(
        execute(&mut state, DeliveryCommand::Reject { delivery_id: id("delivery_one"), generation: None, reason: "no".into() }),
        Err(DeliveryError::StaleClaim)
    ));
    execute(
        &mut state,
        DeliveryCommand::Reject { delivery_id: id("delivery_one"), generation: Some(claim.generation), reason: "no".into() },
    )
    .unwrap();
    execute(&mut state, DeliveryCommand::Block { delivery_id: id("delivery_two"), reason: "recovery".into() }).unwrap();
    assert!(state.queued.is_empty());
    assert_eq!(state.records[&id("delivery_two")].status, DeliveryStatus::Blocked);
}

#[test]
fn tombstones_are_bounded() {
    let mut state = DeliveryProjection::new(1);
    for (delivery, claim) in [("delivery_one", "claim_one"), ("delivery_two", "claim_two")] {
        execute(&mut state, DeliveryCommand::Enqueue { envelope: envelope(delivery, delivery), position: QueuePosition::Back }).unwrap();
        let token = execute(&mut state, DeliveryCommand::Claim { mode: ClaimMode::One, generation: id(claim) }).unwrap().claim.unwrap();
        execute(&mut state, DeliveryCommand::Acknowledge { token }).unwrap();
    }
    assert_eq!(state.tombstones.len(), 1);
    assert_eq!(state.tombstones[0].delivery_id, id("delivery_two"));
}
