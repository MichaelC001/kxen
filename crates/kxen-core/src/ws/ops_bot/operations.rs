use base64::Engine as _;
use serde_json::{Value, json};
use std::sync::Arc;

use crate::AppState;
use crate::bot::memory::{MemoryCommand, MemoryItem, MemoryWrite};
use crate::bot::routine::{RoutineCommand, RoutineWrite};
use crate::bot::run::{ApprovalDecision, RunCommand, RunStatus, RunTrigger, RunTriggerKind};
use crate::bot::system::QueueRun;
use crate::core::identity::{AggregateKind, AggregateRef};

use super::{RpcResult, decode, expected, idempotency, now, owner, resource_id, trace, value};

pub(super) fn handle(method: &str, params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    match method {
        "bot.run.start" => start_run(params, state),
        "bot.run.get" => value(state.bots.runs().get(&resource_id(params, "run_id")?)?),
        "bot.run.list" => list_runs(params, state),
        "bot.run.cancel" => cancel_run(params, state),
        "bot.run.input" => bind_input(params, state),
        "bot.run.approval" => resolve_approval(params, state),
        "bot.routine.list" => routine(method, params, state),
        "bot.routine.create"
        | "bot.routine.update"
        | "bot.routine.pause"
        | "bot.routine.resume"
        | "bot.routine.run_now"
        | "bot.routine.trash"
        | "bot.routine.history" => routine(method, params, state),
        "bot.memory.list" | "bot.memory.create" | "bot.memory.revise" | "bot.memory.remove" => memory(method, params, state),
        "bot.recovery.inspect" | "bot.recovery.repair" | "bot.recovery.clear" => recovery(method, params, state),
        "bot.artifact.get" | "bot.artifact.restore" | "bot.artifact.trash" => artifact(method, params, state),
        _ => Err(format!("unknown Bot operations method: {method}").into()),
    }
}

fn start_run(params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    let input = decode::<Vec<crate::agent::dcp::ProviderNeutralPart>>(params, "input")?;
    value(state.bots.queue_run(QueueRun {
        run_id: resource_id(params, "run_id")?,
        bot_id: resource_id(params, "bot_id")?,
        revision_id: params.get("revision_id").and_then(Value::as_str).map(crate::core::identity::ResourceId::parse).transpose()?,
        trigger: RunTrigger { kind: RunTriggerKind::Manual, source_id: None, occurrence_id: None },
        input,
        conversation_id: params.get("conversation_id").and_then(Value::as_str).map(crate::core::identity::ResourceId::parse).transpose()?,
        task_id: None,
        budget_override: params.get("budget").cloned().map(serde_json::from_value).transpose().map_err(|error| error.to_string())?,
        actor: owner(),
        trace: trace(),
        idempotency_key: idempotency(params)?,
        at_ms: now(),
    })?)
}

fn list_runs(params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    let mut runs = state.bots.runs().list()?;
    if let Some(bot_id) = params.get("bot_id").and_then(Value::as_str) {
        runs.retain(|run| run.spec.bot_id.as_str() == bot_id);
    }
    if let Some(conversation_id) = params.get("conversation_id").and_then(Value::as_str) {
        runs.retain(|run| run.spec.conversation_id.as_ref().is_some_and(|id| id.as_str() == conversation_id));
    }
    if let Some(status) = params.get("status").and_then(Value::as_str) {
        runs.retain(|run| run_status_name(run.status).eq_ignore_ascii_case(status));
    }
    value(runs)
}

fn cancel_run(params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    let run_id = resource_id(params, "run_id")?;
    let current = state.bots.runs().get(&run_id)?;
    if current.status.is_terminal() {
        return value(current);
    }
    let requested = state.bots.runs().execute(crate::bot::run::RunWrite {
        run_id: run_id.clone(),
        expected_version: expected(params)?,
        idempotency_key: idempotency(params)?,
        actor: owner(),
        trace: trace(),
        command: RunCommand::RequestCancel {
            reason: params.get("reason").and_then(Value::as_str).unwrap_or("canceled by owner").into(),
            at_ms: now(),
        },
    })?;
    state.bot_executor.cancel(&run_id);
    value(requested)
}

fn bind_input(params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    let run_id = resource_id(params, "run_id")?;
    value(state.bots.runs().execute(crate::bot::run::RunWrite {
        run_id,
        expected_version: expected(params)?,
        idempotency_key: idempotency(params)?,
        actor: owner(),
        trace: trace(),
        command: RunCommand::BindInput { request_id: resource_id(params, "request_id")?, parts: decode(params, "input")?, at_ms: now() },
    })?)
}

fn resolve_approval(params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    let run_id = resource_id(params, "run_id")?;
    value(state.bots.runs().execute(crate::bot::run::RunWrite {
        run_id,
        expected_version: expected(params)?,
        idempotency_key: idempotency(params)?,
        actor: owner(),
        trace: trace(),
        command: RunCommand::ResolveApproval {
            approval_id: resource_id(params, "approval_id")?,
            decision: if params.get("allow").and_then(Value::as_bool).ok_or("missing allow")? {
                ApprovalDecision::Approved
            } else {
                ApprovalDecision::Denied
            },
            at_ms: now(),
        },
    })?)
}

fn routine(method: &str, params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    if method == "bot.routine.list" {
        let mut routines = state.bots.routines().list()?;
        if let Some(bot_id) = params.get("bot_id").and_then(Value::as_str) {
            routines.retain(|routine| routine.definition.bot_id.as_str() == bot_id);
        }
        return value(routines);
    }
    let routine_id = resource_id(params, "routine_id")?;
    if method == "bot.routine.history" {
        let routine = state.bots.routines().get(&routine_id)?;
        return Ok(json!({ "routine_id": routine_id, "occurrences": routine.occurrences }));
    }
    let command = match method {
        "bot.routine.create" => {
            let definition = decode(params, "definition")?;
            state.bots.validate_routine_definition(&definition)?;
            RoutineCommand::Create { routine_id: routine_id.clone(), definition, at_ms: now() }
        }
        "bot.routine.update" => {
            let definition = decode(params, "definition")?;
            state.bots.validate_routine_definition(&definition)?;
            RoutineCommand::Update { definition, at_ms: now() }
        }
        "bot.routine.pause" => {
            RoutineCommand::Pause { reason: params.get("reason").and_then(Value::as_str).unwrap_or("paused by owner").into(), at_ms: now() }
        }
        "bot.routine.resume" => {
            let routine = state.bots.routines().get(&routine_id)?;
            state.bots.validate_routine_definition(&routine.definition)?;
            RoutineCommand::Resume { at_ms: now() }
        }
        "bot.routine.run_now" => {
            let routine = state.bots.routines().get(&routine_id)?;
            let revision_id = state.bots.validate_routine_definition(&routine.definition)?;
            RoutineCommand::RunNow { occurrence_id: resource_id(params, "occurrence_id")?, resolved_revision_id: revision_id, at_ms: now() }
        }
        "bot.routine.trash" => RoutineCommand::Trash { at_ms: now() },
        _ => return Err(format!("unknown Routine method: {method}").into()),
    };
    value(state.bots.routines().execute(RoutineWrite {
        routine_id,
        expected_version: if method == "bot.routine.create" { 0 } else { expected(params)? },
        idempotency_key: idempotency(params)?,
        actor: owner(),
        trace: trace(),
        command,
    })?)
}

fn memory(method: &str, params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    let bot_id = resource_id(params, "bot_id")?;
    let bot = state.bots.definitions().get(&bot_id)?;
    if method == "bot.memory.list" {
        return value(state.bots.memory().get(&bot_id)?);
    }
    let memory = state.bots.memory().get(&bot_id)?;
    let command = match method {
        "bot.memory.create" => {
            let policy = &bot.current_revision().ok_or("Bot has no published revision")?.definition.memory;
            crate::bot::memory::admit_create(policy, &memory)?;
            MemoryCommand::Create {
                item: MemoryItem {
                    item_id: resource_id(params, "item_id")?,
                    kind: decode(params, "kind")?,
                    content: params.get("content").and_then(Value::as_str).ok_or("missing content")?.into(),
                    provenance: AggregateRef { kind: AggregateKind::Bot, id: bot_id.clone() },
                    version: 1,
                    created_at_ms: now(),
                    updated_at_ms: now(),
                },
            }
        }
        "bot.memory.revise" => MemoryCommand::Revise {
            item_id: resource_id(params, "item_id")?,
            expected_item_version: params.get("expected_item_version").and_then(Value::as_u64).ok_or("missing expected_item_version")?,
            content: params.get("content").and_then(Value::as_str).ok_or("missing content")?.into(),
            at_ms: now(),
        },
        "bot.memory.remove" => MemoryCommand::Remove {
            item_id: resource_id(params, "item_id")?,
            expected_item_version: params.get("expected_item_version").and_then(Value::as_u64).ok_or("missing expected_item_version")?,
            at_ms: now(),
        },
        _ => return Err(format!("unknown Memory method: {method}").into()),
    };
    value(state.bots.memory().execute(MemoryWrite {
        bot_id,
        expected_version: expected(params)?,
        idempotency_key: idempotency(params)?,
        actor: owner(),
        trace: trace(),
        command,
    })?)
}

fn run_status_name(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Queued => "queued",
        RunStatus::Running => "running",
        RunStatus::ApprovalRequired => "approval_required",
        RunStatus::InputRequired => "input_required",
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
        RunStatus::Canceled => "canceled",
        RunStatus::Rejected => "rejected",
        RunStatus::Blocked => "blocked",
    }
}

fn recovery(method: &str, params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    if method == "bot.recovery.inspect" {
        let registry = state.bots.recovery().list_open()?;
        let open_run_ids = registry
            .iter()
            .filter(|record| record.aggregate.kind == AggregateKind::BotRun)
            .map(|record| record.aggregate.id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let bots = state
            .bots
            .definitions()
            .list(true)?
            .into_iter()
            .filter(|bot| bot.lifecycle == crate::bot::BotLifecycle::Blocked)
            .collect::<Vec<_>>();
        let runs = state
            .bots
            .runs()
            .list()?
            .into_iter()
            .filter(|run| run.status == RunStatus::Blocked && open_run_ids.contains(&run.spec.run_id))
            .collect::<Vec<_>>();
        let conversations = state
            .bots
            .conversations()
            .list()?
            .into_iter()
            .filter(|conversation| conversation.lifecycle == crate::bot::conversation::ConversationLifecycle::Blocked)
            .collect::<Vec<_>>();
        let routines = state
            .bots
            .routines()
            .list()?
            .into_iter()
            .filter(|routine| routine.lifecycle == crate::bot::routine::RoutineLifecycle::Blocked)
            .collect::<Vec<_>>();
        return Ok(json!({ "registry": registry, "bots": bots, "runs": runs, "conversations": conversations, "routines": routines }));
    }
    let kind = params.get("kind").and_then(Value::as_str).ok_or("missing kind")?;
    if !matches!(method, "bot.recovery.repair" | "bot.recovery.clear") {
        return Err(format!("unknown Recovery method: {method}").into());
    }
    let _idempotency_key = idempotency(params)?;
    if kind == "bot_run" {
        if method == "bot.recovery.repair" {
            return Err(
                "a BotRun with an UNKNOWN effect has no automatic repair; provide source-specific evidence or clear it as abandoned".into(),
            );
        }
        let run_id = resource_id(params, "aggregate_id")?;
        let run = state.bots.runs().get(&run_id)?;
        if run.event_version != expected(params)? {
            return Err(format!("expected version {}, current version {}", expected(params)?, run.event_version).into());
        }
        if run.status != RunStatus::Blocked {
            return Err("only a blocked BotRun can be cleared from Recovery".into());
        }
        state.bots.recovery().resolve(
            &AggregateRef { kind: AggregateKind::BotRun, id: run_id },
            "owner explicitly abandoned blocked BotRun work; UNKNOWN effect was not retried",
            now(),
        )?;
        return value(run);
    }
    if kind != "bot" {
        return Err(
            "only a blocked Bot definition or BotRun has a Recovery operation; other blocked evidence must be repaired at its source"
                .into(),
        );
    }
    let bot_id = resource_id(params, "aggregate_id")?;
    let bot = state.bots.definitions().change_lifecycle(crate::bot::ChangeLifecycle {
        bot_id: &bot_id,
        expected_event_version: expected(params)?,
        change: crate::bot::LifecycleChange::ClearRecovery,
        actor: owner(),
        trace: trace(),
        idempotency_key: idempotency(params)?,
        at_ms: now(),
    })?;
    state.bots.recovery().resolve(
        &AggregateRef { kind: AggregateKind::Bot, id: bot_id },
        if method == "bot.recovery.repair" { "owner repaired blocked Bot" } else { "owner cleared blocked Bot work" },
        now(),
    )?;
    value(bot)
}

fn artifact(method: &str, params: &Value, state: &Arc<AppState>) -> RpcResult<Value> {
    let artifact_id = resource_id(params, "artifact_id")?;
    match method {
        "bot.artifact.get" => {
            let manifest = state.bots.artifacts().load(&artifact_id)?;
            let content = state
                .bots
                .artifacts()
                .read_verified(&artifact_id, &crate::core::artifact::ArtifactAccess { actor: owner(), conversation_id: None })?;
            Ok(json!({ "manifest": manifest, "content_base64": base64::engine::general_purpose::STANDARD.encode(content) }))
        }
        "bot.artifact.trash" => {
            state.bots.artifacts().trash(&artifact_id)?;
            Ok(json!({ "trashed": true, "artifact_id": artifact_id }))
        }
        "bot.artifact.restore" => value(state.bots.artifacts().restore(&artifact_id)?),
        _ => Err(format!("unknown Artifact method: {method}").into()),
    }
}
