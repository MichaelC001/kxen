use super::super::TeamState;
use crate::agent::agent_loop::AgentContext;
use crate::agent::cancel::CancelToken;
use crate::auth::refresh::RefreshOutcome;
use crate::core::shared::lock;
use crate::llm::ModelRef;
use serde_json::json;
use std::future::Future;
use std::sync::Arc;

pub(super) fn build_ctx(
    state: &Arc<TeamState>,
    runtime: &Arc<crate::workspace_runtime::WorkspaceRuntime>,
    name: &str,
    model: &ModelRef,
    allowed: Option<Vec<String>>,
    cancel: CancelToken,
    wake: u32,
) -> AgentContext {
    let agent_name = name.to_string();
    let session_id = state.session_id.clone();
    let session_id_event = session_id.clone();
    let bus = state.bus.clone();
    let agents = state.deps.agents.clone();
    let agent_name_tx = name.to_string();
    let session_id_tx = session_id.clone();
    // turn 级持久化：每迭代一条 Assistant 消息落 per-member JSONL，id 含成员+wake 维度
    //（确定性，重试/恢复不写双份）；失败经 run_finish fail-closed 终止本轮
    let persist_turn: crate::agent::agent_loop::PersistTurn = {
        let path = super::persist::path(&state.dir, name);
        let session_id_persist = session_id.clone();
        let member = name.to_string();
        let model = model.clone();
        std::sync::Arc::new(move |turn, parts| {
            let mut message = crate::core::session::new_message(&session_id_persist, crate::core::session::Role::Assistant, parts);
            message.id = format!("{member}:w{wake}:t{turn}");
            message.model = Some(model.clone());
            crate::core::session::append_line_idempotent(&path, &message).map_err(|error| error.to_string())
        })
    };
    AgentContext {
        registry: state.deps.registry.clone(),
        tracker: crate::tools::fs_tool::FileTracker::default(),
        workdir: state.workdir.clone(),
        path_grants: Arc::new(std::collections::HashSet::new()),
        model: model.clone(),
        store: lock(&state.deps.store).clone(),
        max_turns: 16,
        mrm: Some(runtime.mrm()),
        allowed_tools: allowed,
        extras: Some(state.deps.extras.extras_for(&state.session_id)),
        hooks: Some(runtime.hooks()),
        loop_detector: crate::agent::loop_detect::LoopDetector::new(),
        cancel: Some(cancel),
        team: state.manager.upgrade(),
        team_identity: Some((session_id.clone(), agent_name.clone())),
        session_id: Some(session_id),
        exec_scope: None,
        bound_goal_id: None,
        goal_binding_frozen: false,
        agents: Some(state.deps.agents.clone()),
        bus: Some(state.bus.clone()),
        approvals: state.deps.approvals.clone(),
        kanban_auto: None,
        mcp: Some(runtime.mcp()),
        lsp: Some(runtime.lsp()),
        notify: None,
        persist_compaction: None,
        persist_turn: Some(persist_turn),
        auxiliary_usage: Arc::default(),
        usage_reporter: Some(usage_reporter(state)),
        stream_override: None,
        on_event: Arc::new(move |event| {
            let mut payload = match serde_json::to_value(&event) {
                Ok(value) => value,
                Err(_) => return,
            };
            if let Some(object) = payload.as_object_mut() {
                object.insert("agent".into(), json!(agent_name));
                object.insert("session_id".into(), json!(session_id_event));
            }
            agents.push_transcript(&session_id_tx, &agent_name_tx, payload.clone());
            bus.publish(crate::core::event::Event::LlmDelta(payload));
        }),
    }
}

fn usage_reporter(state: &Arc<TeamState>) -> crate::agent::agent_loop::UsageReporter {
    crate::agent::agent_loop::UsageReporter::new(state.session_id.clone(), state.deps.session_usage.clone(), state.bus.clone())
}

pub(crate) fn teammate_system(state: &Arc<TeamState>, name: &str, role: &str, approved: bool) -> String {
    let mode = if approved {
        "You may use your full tool set to implement."
    } else {
        "You are in PLAN-ONLY mode: read-only tools. Produce a concrete plan and stop - the lead must approve it before you implement anything."
    };
    // roster 每轮重建：成员状态变化（新 spawn / shutdown / 状态流转）实时反映进 system prompt
    let roster = lock(&state.members)
        .iter()
        .map(|m| format!("- {} (role: {}, model: {}, status: {:?})", m.name, m.role, m.model.model, m.status))
        .collect::<Vec<_>>()
        .join("\n");
    let observer_note = if role == "observer" {
        " You are the OBSERVER: you receive copies of all team traffic. Watch the process and report summaries or issues to the lead."
    } else {
        ""
    };
    format!(
        "You are teammate \"{name}\" (role: {role}) in a kxen agent team. {mode}{observer_note} \
        Current team roster:\n{roster}\n\
        Coordinate via send_message (to: \"lead\" or a teammate name from the roster) and team_task (claim/complete/list). \
        Act on every task brief IMMEDIATELY with tools (write/edit/exec/read) in the SAME turn - never reply with intent-only text such as \"I will start\". \
        Report results to the lead when done, then go idle."
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CredentialRefresh {
    Finished(RefreshOutcome),
    Cancelled,
    GoalStopped,
}

pub(super) async fn refresh_store_credentials(state: &Arc<TeamState>, model: &ModelRef, cancel: &CancelToken) -> CredentialRefresh {
    refresh_store_credentials_in(state, model, cancel, &crate::core::paths::goals_dir()).await
}

pub(super) async fn refresh_store_credentials_in(
    state: &Arc<TeamState>,
    model: &ModelRef,
    cancel: &CancelToken,
    goals_dir: &std::path::Path,
) -> CredentialRefresh {
    if cancel.is_cancelled() {
        return CredentialRefresh::Cancelled;
    }
    let remaining = match goal_refresh_budget_in(goals_dir, &state.session_id) {
        crate::core::goal::RuntimeBudget::Unbounded => None,
        crate::core::goal::RuntimeBudget::WallRemaining(remaining) => Some(remaining),
        crate::core::goal::RuntimeBudget::Stop(_) => return CredentialRefresh::GoalStopped,
    };
    let mut store = lock(&state.deps.store).clone();
    let refresh = crate::auth::refresh::ensure_fresh(&mut store, &model.provider, model.account.as_deref());
    let outcome = wait_for_refresh(refresh, cancel, remaining).await;
    if outcome == CredentialRefresh::Finished(RefreshOutcome::Refreshed) {
        write_back_credential(&state.deps.store, &model.provider, model.account.as_deref(), &store);
    }
    outcome
}

pub(super) fn goal_refresh_budget_in(goals_dir: &std::path::Path, session_id: &str) -> crate::core::goal::RuntimeBudget {
    match crate::core::goal::Goal::focus_for_checked(goals_dir, Some(session_id)) {
        Ok(Some(goal)) => goal.runtime_budget(crate::core::shared::now_ms()),
        Ok(None) => crate::core::goal::RuntimeBudget::Unbounded,
        Err(error) => {
            tracing::error!(%error, "teammate goal state load failed");
            crate::core::goal::RuntimeBudget::Stop(crate::core::goal::GoalStatus::Blocked)
        }
    }
}

pub(super) async fn wait_for_refresh<F>(refresh: F, cancel: &CancelToken, remaining: Option<std::time::Duration>) -> CredentialRefresh
where
    F: Future<Output = RefreshOutcome>,
{
    let deadline = async {
        match remaining {
            Some(remaining) => tokio::time::sleep(remaining).await,
            None => std::future::pending::<()>().await,
        }
    };
    tokio::select! {
        biased;
        _ = cancel.wait() => CredentialRefresh::Cancelled,
        _ = deadline => CredentialRefresh::GoalStopped,
        refreshed = refresh => CredentialRefresh::Finished(refreshed),
    }
}

pub(super) fn write_back_credential(
    store: &Arc<std::sync::Mutex<crate::auth::credential::AuthStore>>,
    provider: &str,
    account: Option<&str>,
    refreshed: &crate::auth::credential::AuthStore,
) {
    let key = account.map(|value| crate::auth::credential::account_id(provider, value)).unwrap_or_else(|| provider.to_string());
    if let Some(credential) = refreshed.get(&key).cloned() {
        lock(store).insert(key, credential);
    }
}
