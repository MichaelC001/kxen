use super::*;
use crate::agent::dcp::{ProviderNeutralPart, TurnRecord, TurnRecordKind};
use crate::core::identity::{ActorRef, ContentHash, IdempotencyKey, ResourceId, TraceContext};
use crate::core::operation::OperationOutcome;

pub(super) fn id(value: &str) -> ResourceId {
    ResourceId::parse(value).unwrap()
}

pub(super) fn repository(name: &str) -> RunRepository {
    RunRepository::new(std::env::temp_dir().join(format!("kxen-run-{name}-{}", uuid::Uuid::new_v4())))
}

pub(super) fn spec(run_id: ResourceId) -> RunSpec {
    RunSpec {
        run_id,
        bot_id: id("bot_worker"),
        revision_id: id("brev_one"),
        revision_hash: ContentHash::from_bytes(b"revision"),
        mrm_role: id("execution"),
        trigger: RunTrigger { kind: RunTriggerKind::Manual, source_id: None, occurrence_id: None },
        input: vec![ProviderNeutralPart::Text { text: "do work".into() }],
        conversation_id: None,
        task_id: None,
        permission: PermissionSnapshot {
            capabilities: Default::default(),
            resources: Default::default(),
            approval: crate::bot::ApprovalPolicy::ManualWhenRequired,
            budget: Default::default(),
        },
    }
}

pub(super) fn write(repo: &RunRepository, run_id: &ResourceId, expected: u64, key: &str, command: RunCommand) -> BotRunState {
    repo.execute(RunWrite {
        run_id: run_id.clone(),
        expected_version: expected,
        idempotency_key: IdempotencyKey::parse(key).unwrap(),
        actor: ActorRef::Owner,
        trace: TraceContext::default(),
        command,
    })
    .unwrap()
}

pub(super) fn queued(repo: &RunRepository, run_id: &ResourceId) -> BotRunState {
    write(repo, run_id, 0, "idem_queue", RunCommand::Queue { spec: Box::new(spec(run_id.clone())), at_ms: 10 })
}

#[test]
fn run_has_durable_turns_and_terminal() {
    let repo = repository("complete");
    let run_id = id("brun_complete");
    let queued = queued(&repo, &run_id);
    let running = write(&repo, &run_id, queued.event_version, "idem_start", RunCommand::Start { at_ms: 20 });
    let turn = TurnRecord {
        record_id: id("turn_one"),
        kind: TurnRecordKind::Response,
        parts: vec![ProviderNeutralPart::Text { text: "checked result".into() }],
        created_at_ms: 30,
    };
    let recorded = write(&repo, &run_id, running.event_version, "idem_turn", RunCommand::RecordTurn { record: turn, at_ms: 30 });
    let completed = write(
        &repo,
        &run_id,
        recorded.event_version,
        "idem_complete",
        RunCommand::Complete {
            result: vec![ProviderNeutralPart::Text { text: "checked result".into() }],
            usage: UsageSummary { turns: 1, ..Default::default() },
            at_ms: 40,
        },
    );
    assert_eq!(completed.status, RunStatus::Completed);
    assert_eq!(repo.get(&run_id).unwrap(), completed);
    assert!(repo.recoverable().unwrap().is_empty());
    std::fs::remove_dir_all(repo.root()).ok();
}

#[test]
fn unknown_tool_outcome_blocks_without_retry() {
    let repo = repository("unknown");
    let run_id = id("brun_unknown");
    let running = write(&repo, &run_id, queued(&repo, &run_id).event_version, "idem_start", RunCommand::Start { at_ms: 20 });
    let operation_id = id("op_write");
    let generation = id("gen_one");
    let prepared = write(
        &repo,
        &run_id,
        running.event_version,
        "idem_prepare",
        RunCommand::PrepareTool {
            operation_id: operation_id.clone(),
            generation: generation.clone(),
            intent: ToolIntent { call_id: id("call_one"), capability_id: id("write"), arguments_json: "{\"path\":\"out.txt\"}".into() },
            at_ms: 30,
        },
    );
    let started = write(
        &repo,
        &run_id,
        prepared.event_version,
        "idem_started",
        RunCommand::MarkToolStarted { operation_id: operation_id.clone(), generation: generation.clone(), at_ms: 40 },
    );
    let blocked = write(
        &repo,
        &run_id,
        started.event_version,
        "idem_unknown",
        RunCommand::MarkToolUnknown {
            operation_id: operation_id.clone(),
            generation,
            reason: "process crashed after invocation".into(),
            evidence: Vec::new(),
            at_ms: 50,
        },
    );
    assert_eq!(blocked.status, RunStatus::Blocked);
    assert!(blocked.tool_operations[&operation_id].attempt.as_ref().unwrap().unknown_reason.is_some());
    let retry = repo.execute(RunWrite {
        run_id: run_id.clone(),
        expected_version: blocked.event_version,
        idempotency_key: IdempotencyKey::parse("idem_retry").unwrap(),
        actor: ActorRef::Owner,
        trace: TraceContext::default(),
        command: RunCommand::RecordToolOutcome {
            operation_id,
            generation: id("gen_one"),
            outcome: OperationOutcome::Succeeded { value: ToolExecutionResult { output: "maybe".into(), is_error: false } },
            evidence: Vec::new(),
            at_ms: 60,
        },
    });
    assert!(matches!(retry, Err(RunError::Transition(_))));
    std::fs::remove_dir_all(repo.root()).ok();
}

#[test]
fn started_tool_cannot_be_hidden_by_failed_terminal() {
    let repo = repository("uncertain-fail");
    let run_id = id("brun_uncertain_fail");
    let running = write(&repo, &run_id, queued(&repo, &run_id).event_version, "idem_uncertain_start", RunCommand::Start { at_ms: 20 });
    let operation_id = id("op_uncertain_write");
    let generation = id("gen_uncertain");
    let prepared = write(
        &repo,
        &run_id,
        running.event_version,
        "idem_uncertain_prepare",
        RunCommand::PrepareTool {
            operation_id: operation_id.clone(),
            generation: generation.clone(),
            intent: ToolIntent { call_id: id("call_uncertain"), capability_id: id("write"), arguments_json: "{}".into() },
            at_ms: 30,
        },
    );
    let started = write(
        &repo,
        &run_id,
        prepared.event_version,
        "idem_uncertain_started",
        RunCommand::MarkToolStarted { operation_id, generation, at_ms: 40 },
    );
    let failed = repo.execute(RunWrite {
        run_id: run_id.clone(),
        expected_version: started.event_version,
        idempotency_key: IdempotencyKey::parse("idem_uncertain_failed").unwrap(),
        actor: ActorRef::Owner,
        trace: TraceContext::default(),
        command: RunCommand::Fail {
            code: "runtime_error".into(),
            message: "transport failed".into(),
            usage: Default::default(),
            at_ms: 50,
        },
    });
    assert!(matches!(failed, Err(RunError::Transition(_))));
    let canceled = repo.execute(RunWrite {
        run_id: run_id.clone(),
        expected_version: started.event_version,
        idempotency_key: IdempotencyKey::parse("idem_uncertain_canceled").unwrap(),
        actor: ActorRef::Owner,
        trace: TraceContext::default(),
        command: RunCommand::Cancel { reason: "owner canceled".into(), usage: Default::default(), at_ms: 50 },
    });
    assert!(matches!(canceled, Err(RunError::Transition(_))));
    std::fs::remove_dir_all(repo.root()).ok();
}

#[test]
fn running_cancel_is_durably_requested_before_terminal_cancellation() {
    let repo = repository("cancel-request");
    let run_id = id("brun_cancel_request");
    let running = write(&repo, &run_id, queued(&repo, &run_id).event_version, "idem_cancel_start", RunCommand::Start { at_ms: 20 });
    let requested = write(
        &repo,
        &run_id,
        running.event_version,
        "idem_cancel_request",
        RunCommand::RequestCancel { reason: "owner canceled".into(), at_ms: 30 },
    );
    assert_eq!(requested.status, RunStatus::Running);
    assert_eq!(requested.cancellation_requested.as_deref(), Some("owner canceled"));
    let late_failure = repo.execute(RunWrite {
        run_id: run_id.clone(),
        expected_version: requested.event_version,
        idempotency_key: IdempotencyKey::parse("idem_late_failure").unwrap(),
        actor: ActorRef::Owner,
        trace: TraceContext::default(),
        command: RunCommand::Fail {
            code: "late_failure".into(),
            message: "must not overtake cancellation".into(),
            usage: Default::default(),
            at_ms: 35,
        },
    });
    assert!(matches!(late_failure, Err(RunError::Transition(_))));
    let canceled = write(
        &repo,
        &run_id,
        requested.event_version,
        "idem_cancel_terminal",
        RunCommand::Cancel { reason: "owner canceled".into(), usage: Default::default(), at_ms: 40 },
    );
    assert_eq!(canceled.status, RunStatus::Canceled);
    assert!(canceled.cancellation_requested.is_none());
    std::fs::remove_dir_all(repo.root()).ok();
}

#[test]
fn known_tool_outcome_must_settle_before_failed_terminal() {
    let repo = repository("known-unsettled-fail");
    let run_id = id("brun_known_unsettled_fail");
    let running = write(&repo, &run_id, queued(&repo, &run_id).event_version, "idem_known_start", RunCommand::Start { at_ms: 20 });
    let operation_id = id("op_known_write");
    let generation = id("gen_known");
    let prepared = write(
        &repo,
        &run_id,
        running.event_version,
        "idem_known_prepare",
        RunCommand::PrepareTool {
            operation_id: operation_id.clone(),
            generation: generation.clone(),
            intent: ToolIntent { call_id: id("call_known"), capability_id: id("write"), arguments_json: "{}".into() },
            at_ms: 30,
        },
    );
    let started = write(
        &repo,
        &run_id,
        prepared.event_version,
        "idem_known_started",
        RunCommand::MarkToolStarted { operation_id: operation_id.clone(), generation: generation.clone(), at_ms: 40 },
    );
    let known = write(
        &repo,
        &run_id,
        started.event_version,
        "idem_known_outcome",
        RunCommand::RecordToolOutcome {
            operation_id,
            generation,
            outcome: OperationOutcome::Succeeded { value: ToolExecutionResult { output: "committed".into(), is_error: false } },
            evidence: Vec::new(),
            at_ms: 50,
        },
    );
    let failed = repo.execute(RunWrite {
        run_id: run_id.clone(),
        expected_version: known.event_version,
        idempotency_key: IdempotencyKey::parse("idem_known_failed").unwrap(),
        actor: ActorRef::Owner,
        trace: TraceContext::default(),
        command: RunCommand::Fail {
            code: "runtime_error".into(),
            message: "settlement interrupted".into(),
            usage: Default::default(),
            at_ms: 60,
        },
    });
    assert!(matches!(failed, Err(RunError::Transition(_))));
    std::fs::remove_dir_all(repo.root()).ok();
}

mod interruptions;

#[test]
fn queued_and_running_runs_are_discovered_after_restart() {
    let repo = repository("recovery");
    let one = id("brun_queued");
    let two = id("brun_running");
    queued(&repo, &one);
    let second = queued(&repo, &two);
    write(&repo, &two, second.event_version, "idem_start_two", RunCommand::Start { at_ms: 30 });
    let recovered = repo.recoverable().unwrap();
    assert_eq!(recovered.len(), 2);
    assert!(recovered.iter().any(|run| run.status == RunStatus::Queued));
    assert!(recovered.iter().any(|run| run.status == RunStatus::Running));
    std::fs::remove_dir_all(repo.root()).ok();
}
