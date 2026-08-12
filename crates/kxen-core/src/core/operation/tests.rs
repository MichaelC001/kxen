use super::*;
use crate::core::identity::ResourceId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Intent {
    command: String,
}

fn id(value: &str) -> ResourceId {
    ResourceId::parse(value).unwrap()
}

fn execute(
    state: &mut OperationProjection<Intent, String>,
    command: OperationCommand<Intent, String>,
) -> Result<OperationDecision<Intent, String>, OperationError> {
    let decision = state.decide(command)?;
    for event in decision.events.clone() {
        state.apply(event)?;
    }
    Ok(decision)
}

fn prepared() -> OperationProjection<Intent, String> {
    let mut state = OperationProjection::default();
    execute(
        &mut state,
        OperationCommand::Prepare {
            operation_id: id("op_test"),
            generation: id("attempt_one"),
            intent: Intent { command: "write".into() },
            at_ms: 1,
        },
    )
    .unwrap();
    state
}

#[test]
fn prepare_is_idempotent_but_changed_intent_collides() {
    let mut state = prepared();
    let same = OperationCommand::Prepare {
        operation_id: id("op_test"),
        generation: id("attempt_one"),
        intent: Intent { command: "write".into() },
        at_ms: 2,
    };
    assert!(execute(&mut state, same).unwrap().duplicate);
    let changed = OperationCommand::Prepare {
        operation_id: id("op_test"),
        generation: id("attempt_one"),
        intent: Intent { command: "delete".into() },
        at_ms: 2,
    };
    assert!(matches!(execute(&mut state, changed), Err(OperationError::Collision(_))));
}

#[test]
fn known_outcome_settles_after_durable_start() {
    let mut state = prepared();
    execute(&mut state, OperationCommand::MarkStarted { generation: id("attempt_one"), at_ms: 2 }).unwrap();
    execute(
        &mut state,
        OperationCommand::RecordOutcome {
            generation: id("attempt_one"),
            outcome: OperationOutcome::Succeeded { value: "done".into() },
            evidence: Vec::new(),
        },
    )
    .unwrap();
    execute(&mut state, OperationCommand::Settle { generation: id("attempt_one"), at_ms: 3 }).unwrap();
    assert_eq!(state.attempt.unwrap().phase, AttemptPhase::Settled);
}

#[test]
fn unknown_outcome_cannot_settle_until_recovered() {
    let mut state = prepared();
    execute(&mut state, OperationCommand::MarkStarted { generation: id("attempt_one"), at_ms: 2 }).unwrap();
    execute(
        &mut state,
        OperationCommand::MarkOutcomeUnknown { generation: id("attempt_one"), reason: "result commit failed".into(), evidence: Vec::new() },
    )
    .unwrap();
    assert!(matches!(
        execute(&mut state, OperationCommand::Settle { generation: id("attempt_one"), at_ms: 3 }),
        Err(OperationError::InvalidTransition { from: AttemptPhase::OutcomeUnknown, .. })
    ));
    execute(
        &mut state,
        OperationCommand::RecordOutcome {
            generation: id("attempt_one"),
            outcome: OperationOutcome::Failed { code: "external".into(), message: "verified failed".into() },
            evidence: Vec::new(),
        },
    )
    .unwrap();
    execute(&mut state, OperationCommand::Settle { generation: id("attempt_one"), at_ms: 4 }).unwrap();
    assert_eq!(state.attempt.unwrap().phase, AttemptPhase::Settled);
}

#[test]
fn stale_generation_cannot_change_attempt() {
    let mut state = prepared();
    assert!(matches!(
        execute(&mut state, OperationCommand::MarkStarted { generation: id("attempt_stale"), at_ms: 2 }),
        Err(OperationError::StaleGeneration)
    ));
    assert_eq!(state.attempt.unwrap().phase, AttemptPhase::Prepared);
}

#[test]
fn cancel_is_allowed_only_before_side_effect_start() {
    let mut state = prepared();
    execute(&mut state, OperationCommand::CancelBeforeStart { generation: id("attempt_one"), at_ms: 2 }).unwrap();
    assert_eq!(state.attempt.unwrap().phase, AttemptPhase::CanceledBeforeStart);

    let mut started = prepared();
    execute(&mut started, OperationCommand::MarkStarted { generation: id("attempt_one"), at_ms: 2 }).unwrap();
    assert!(matches!(
        execute(&mut started, OperationCommand::CancelBeforeStart { generation: id("attempt_one"), at_ms: 3 }),
        Err(OperationError::InvalidTransition { from: AttemptPhase::Started, .. })
    ));
}

#[test]
fn replay_rejects_tampered_intent_hash() {
    let mut state = OperationProjection::<Intent, String>::default();
    let event = OperationEvent::Prepared {
        operation_id: id("op_test"),
        generation: id("attempt_one"),
        intent: Intent { command: "write".into() },
        intent_hash: ContentHash::from_bytes(b"wrong"),
        at_ms: 1,
    };
    assert!(matches!(state.apply(event), Err(OperationError::IntentHashMismatch)));
}
