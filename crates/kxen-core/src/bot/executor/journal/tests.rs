use super::*;
use crate::agent::capability::CapabilitySet;
use crate::agent::dcp::{ProviderNeutralPart, ToolBoundaryJournal};
use crate::agent::runtime::ExecutionBudget;
use crate::bot::run::{PermissionSnapshot, RunSpec, RunTrigger, RunTriggerKind, RunWrite};
use crate::bot::{ApprovalPolicy, ResourcePolicy};
use crate::core::identity::{ActorRef, ContentHash, IdempotencyKey, TraceContext};

fn id(value: &str) -> ResourceId {
    ResourceId::parse(value).unwrap()
}

fn write(system: &BotSystem, run_id: &ResourceId, expected_version: u64, key: &str, command: RunCommand) {
    system
        .runs()
        .execute(RunWrite {
            run_id: run_id.clone(),
            expected_version,
            idempotency_key: IdempotencyKey::parse(key).unwrap(),
            actor: ActorRef::System { actor: SystemActor::Runtime },
            trace: TraceContext::default(),
            command,
        })
        .unwrap();
}

#[test]
fn repeated_identical_tool_calls_receive_distinct_durable_operations() {
    let root = std::env::temp_dir().join(format!("kxen-bot-journal-repeat-{}", uuid::Uuid::new_v4()));
    let system = Arc::new(BotSystem::new(&root).unwrap());
    let run_id = id("brun_repeated_calls");
    let spec = RunSpec {
        run_id: run_id.clone(),
        bot_id: id("bot_repeated_calls"),
        revision_id: id("brev_repeated_calls"),
        revision_hash: ContentHash::from_bytes(b"revision"),
        mrm_role: id("execution"),
        trigger: RunTrigger { kind: RunTriggerKind::Manual, source_id: None, occurrence_id: None },
        input: vec![ProviderNeutralPart::Text { text: "read twice".into() }],
        conversation_id: None,
        task_id: None,
        permission: PermissionSnapshot {
            capabilities: CapabilitySet::new([id("read")]),
            resources: ResourcePolicy::default(),
            approval: ApprovalPolicy::ManualWhenRequired,
            budget: ExecutionBudget::default(),
        },
    };
    write(&system, &run_id, 0, "idem_repeat_queue", RunCommand::Queue { spec: Box::new(spec), at_ms: 1 });
    write(&system, &run_id, 1, "idem_repeat_start", RunCommand::Start { at_ms: 2 });

    let journal = RunToolJournal::new(system.clone(), run_id.clone());
    assert_eq!(journal.before("provider_call_one", "read", r#"{"path":"same.txt"}"#, 3).unwrap(), ToolBoundaryAction::Execute);
    journal.after("provider_call_one", "read", r#"{"path":"same.txt"}"#, "first", false, 4).unwrap();
    assert!(journal.before("provider_call_one", "read", r#"{"path":"different.txt"}"#, 5).is_err());
    assert_eq!(journal.before("provider_call_two", "read", r#"{"path":"same.txt"}"#, 5).unwrap(), ToolBoundaryAction::Execute);
    journal.after("provider_call_two", "read", r#"{"path":"same.txt"}"#, "second", false, 6).unwrap();

    let run = system.runs().get(&run_id).unwrap();
    assert_eq!(run.tool_operations.len(), 2);
    assert!(
        run.tool_operations
            .values()
            .all(|operation| { operation.attempt.as_ref().is_some_and(|attempt| attempt.phase == AttemptPhase::Settled) })
    );
    let outputs = run
        .tool_operations
        .values()
        .filter_map(|operation| operation.attempt.as_ref()?.outcome.as_ref())
        .filter_map(|outcome| match outcome {
            OperationOutcome::Succeeded { value } => Some(value.output.as_str()),
            OperationOutcome::Failed { .. } => None,
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(outputs, std::collections::BTreeSet::from(["first", "second"]));
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn connector_tools_always_cross_the_durable_bot_approval_boundary() {
    let root = std::env::temp_dir().join(format!("kxen-bot-journal-mcp-{}", uuid::Uuid::new_v4()));
    let system = Arc::new(BotSystem::new(&root).unwrap());
    let run_id = id("brun_mcp_approval");
    let mut resources = ResourcePolicy::default();
    resources.connectors.insert(id("docs"));
    let spec = RunSpec {
        run_id: run_id.clone(),
        bot_id: id("bot_mcp_approval"),
        revision_id: id("brev_mcp_approval"),
        revision_hash: ContentHash::from_bytes(b"revision"),
        mrm_role: id("execution"),
        trigger: RunTrigger { kind: RunTriggerKind::Manual, source_id: None, occurrence_id: None },
        input: vec![ProviderNeutralPart::Text { text: "read connector".into() }],
        conversation_id: None,
        task_id: None,
        permission: PermissionSnapshot {
            capabilities: CapabilitySet::default(),
            resources,
            approval: ApprovalPolicy::ManualWhenRequired,
            budget: ExecutionBudget::default(),
        },
    };
    write(&system, &run_id, 0, "idem_mcp_queue", RunCommand::Queue { spec: Box::new(spec), at_ms: 1 });
    write(&system, &run_id, 1, "idem_mcp_start", RunCommand::Start { at_ms: 2 });

    let journal = RunToolJournal::new(system.clone(), run_id.clone());
    let action = journal.before("provider_mcp_call", "mcp__docs__search", r#"{"query":"evidence"}"#, 3).unwrap();
    assert!(matches!(action, ToolBoundaryAction::Pause { .. }));
    let run = system.runs().get(&run_id).unwrap();
    assert_eq!(run.status, crate::bot::run::RunStatus::ApprovalRequired);
    assert!(run.approval.as_ref().is_some_and(|request| request.operation_id.is_some()));
    std::fs::remove_dir_all(root).ok();
}
