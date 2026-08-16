use std::sync::Arc;

use super::*;
use crate::agent::agent_loop::{AgentEvent, AgentOutcome, RunStats};
use crate::bot::run::{RunCommand, RunWrite};
use crate::bot::system::QueueRun;
use crate::bot::{BotDefinition, CreateBot, PublishBot};
use crate::core::identity::{ActorRef, IdempotencyKey, ResourceId, TraceContext};

fn id(value: &str) -> ResourceId {
    ResourceId::parse(value).unwrap()
}

fn key(value: &str) -> IdempotencyKey {
    IdempotencyKey::parse(value).unwrap()
}

fn executor_fixture(name: &str) -> (Arc<BotSystem>, BotExecutor, ResourceId, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("kxen-bot-executor-{name}-{}", uuid::Uuid::new_v4()));
    let system = Arc::new(BotSystem::new(&root).unwrap());
    let bot_id = id(&format!("bot_executor_{name}"));
    let mut definition = BotDefinition::empty("Executor Bot");
    definition.objective = "Produce evidence".into();
    definition.instructions = "Return exact output".into();
    definition.success_criteria = vec!["Evidence exists".into()];
    definition.output_contract.description = "Evidence".into();
    if name == "contract" {
        definition.output_contract.content_type = "application/json".into();
        definition.output_contract.required_fields = vec!["status".into()];
    }
    if name == "budget" {
        definition.budget.max_tokens = Some(1);
    }
    let created = system
        .definitions()
        .create(CreateBot {
            bot_id: &bot_id,
            definition: &definition,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key(&format!("idem_executor_create_{name}")),
            at_ms: 1,
        })
        .unwrap();
    let draft = created.draft.unwrap();
    system
        .definitions()
        .publish(PublishBot {
            bot_id: &bot_id,
            expected_event_version: created.event_version,
            expected_draft_version: draft.version,
            expected_content_hash: &draft.content_hash,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key(&format!("idem_executor_publish_{name}")),
            at_ms: 2,
        })
        .unwrap();
    let run_id = id(&format!("brun_executor_{name}"));
    system
        .queue_run(QueueRun {
            run_id: run_id.clone(),
            bot_id,
            revision_id: None,
            trigger: crate::bot::run::RunTrigger { kind: crate::bot::run::RunTriggerKind::Manual, source_id: None, occurrence_id: None },
            input: vec![crate::agent::dcp::ProviderNeutralPart::Text { text: "make evidence".into() }],
            conversation_id: None,
            task_id: None,
            budget_override: None,
            actor: ActorRef::Owner,
            trace: TraceContext::default(),
            idempotency_key: key(&format!("idem_executor_queue_{name}")),
            at_ms: 3,
        })
        .unwrap();
    let executor = BotExecutor::new(
        system.clone(),
        BotExecutorDeps {
            registry: Arc::new(crate::tools::task::TaskRegistry::new()),
            auth_store: Arc::new(std::sync::Mutex::new(Arc::new(Default::default()))),
            runtimes: Arc::new(crate::workspace_runtime::WorkspaceRuntimeRegistry::default()),
            session_usage: Arc::new(std::sync::Mutex::new(Default::default())),
            bus: crate::core::event::EventBus::default(),
        },
    );
    (system, executor, run_id, root)
}

fn start(system: &BotSystem, run_id: &ResourceId, suffix: &str) {
    let queued = system.runs().get(run_id).unwrap();
    system
        .runs()
        .execute(RunWrite {
            run_id: run_id.clone(),
            expected_version: queued.event_version,
            idempotency_key: key(&format!("idem_executor_start_{suffix}")),
            actor: ActorRef::System { actor: crate::core::identity::SystemActor::Runtime },
            trace: TraceContext::default(),
            command: RunCommand::Start { at_ms: 4 },
        })
        .unwrap();
}

fn outcome(text: &str, aborted: bool, stats: Option<RunStats>) -> AgentOutcome {
    AgentOutcome {
        final_text: text.into(),
        turns: 2,
        aborted,
        stats,
        terminal: if aborted { AgentEvent::Aborted } else { AgentEvent::Done { turns: 2, stats } },
        provider_model: None,
    }
}

#[test]
fn messages_preserve_context_history_resume_and_tool_pairs() {
    use crate::agent::dcp::{ContextFrame, ContextLayer, ContextSegment, ProviderNeutralPart, TurnRecord, TurnRecordKind, VisibilityRef};
    use crate::core::session::Part;

    let run_id = id("brun_message_conversion");
    let frame = ContextFrame {
        source_version: crate::core::identity::ContentHash::from_bytes(b"frame"),
        segments: vec![
            ContextSegment {
                stable_id: id("ctx_system"),
                layer: ContextLayer::Definition,
                order_key: "1".into(),
                visibility: VisibilityRef::Run { run_id: run_id.clone() },
                parts: vec![ProviderNeutralPart::Text { text: "system block".into() }],
            },
            ContextSegment {
                stable_id: id("ctx_input"),
                layer: ContextLayer::NewInput,
                order_key: "2".into(),
                visibility: VisibilityRef::Run { run_id: run_id.clone() },
                parts: vec![ProviderNeutralPart::Text { text: "input block".into() }],
            },
        ],
    };
    let mut wire = messages::from_context(&frame);
    let parts = messages::session_parts(
        &run_id,
        1,
        vec![
            Part::Text { text: "answer".into() },
            Part::ToolCall {
                name: "read".into(),
                input: serde_json::json!({}),
                output: "ERROR: denied".into(),
                args: Some(serde_json::json!({ "path": "file.txt" })),
                id: None,
                started_at: None,
                finished_at: None,
            },
            Part::Reasoning { text: "reason".into() },
        ],
    )
    .unwrap();
    messages::append_history(
        &mut wire,
        &[TurnRecord { record_id: id("turn_message_conversion"), kind: TurnRecordKind::Response, parts, created_at_ms: 1 }],
    );
    let (system, _executor, fixture_run_id, root) = executor_fixture("message");
    let mut run = system.runs().get(&fixture_run_id).unwrap();
    run.bound_inputs = vec![ProviderNeutralPart::Text { text: "owner input".into() }];
    run.approved_operations.insert(id("operation_approved"));
    messages::append_resume_state(&mut wire, &run);
    assert_eq!(wire.len(), 6);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn finish_maps_success_contract_failure_budget_and_abort() {
    let stats = RunStats {
        ttft_ms: 1,
        duration_ms: 5,
        input_tokens: 2,
        output_tokens: 3,
        unmetered_calls: 0,
        usage_complete: true,
        last_input_tokens: 2,
        tokens_per_sec: 1,
    };

    let (system, executor, run_id, root) = executor_fixture("success");
    start(&system, &run_id, "success");
    let completed = executor.finish(&run_id, outcome("verified", false, Some(stats))).unwrap();
    assert_eq!(completed.status, crate::bot::run::RunStatus::Completed);
    std::fs::remove_dir_all(root).ok();

    let (system, executor, run_id, root) = executor_fixture("contract");
    start(&system, &run_id, "contract");
    let failed = executor.finish(&run_id, outcome("not-json", false, Some(stats))).unwrap();
    assert_eq!(failed.error_code.as_deref(), Some("output_contract_violation"));
    std::fs::remove_dir_all(root).ok();

    let (system, executor, run_id, root) = executor_fixture("budget");
    start(&system, &run_id, "budget");
    let failed = executor.finish(&run_id, outcome("verified", false, Some(stats))).unwrap();
    assert_eq!(failed.error_code.as_deref(), Some("budget_exceeded"));
    std::fs::remove_dir_all(root).ok();

    let (system, executor, run_id, root) = executor_fixture("abort");
    start(&system, &run_id, "abort");
    let canceled = executor.finish(&run_id, outcome("partial", true, Some(stats))).unwrap();
    assert_eq!(canceled.status, crate::bot::run::RunStatus::Canceled);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn executor_short_circuits_terminal_and_records_runtime_failure() {
    let (system, executor, run_id, root) = executor_fixture("runtime_failure");
    assert!(!executor.cancel(&run_id));
    executor.persist_execution_error(&run_id, "runtime unavailable");
    let failed = system.runs().get(&run_id).unwrap();
    assert_eq!(failed.error_code.as_deref(), Some("runtime_execution_error"));
    assert_eq!(local_sandbox(&run_id).file_name().unwrap(), run_id.as_str());
    std::fs::remove_dir_all(root).ok();
}
