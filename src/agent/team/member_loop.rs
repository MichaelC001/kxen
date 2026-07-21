// ---------------- teammate 常驻 loop ----------------

use crate::agent::agent_loop::{run_turn, AgentContext};
use crate::agent::cancel::CancelToken;
use crate::core::shared::lock;
use crate::llm::{Message, ModelRef};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Notify;

use super::inbox::{append_inbox, drain_inbox};
use super::types::MemberStatus;
use super::TeamState;

pub(super) async fn teammate_loop(
    state: Arc<TeamState>,
    name: String,
    role: String,
    model: ModelRef,
    prompt: String,
    plan_approval: bool,
    cancel: CancelToken,
    notify: Arc<Notify>,
) {
    let mut phase_prompt = prompt;
    let mut approved = !plan_approval;
    loop {
        if cancel.is_cancelled() {
            break;
        }
        set_status(&state, &name, MemberStatus::Working);
        // 阶段 ctx：plan_approval 未批准前只读
        let allowed: Option<&'static [&'static str]> = if approved { None } else { Some(READONLY_TEAM_TOOLS) };
        let mut ctx = build_ctx(&state, &name, &role, &model, allowed, cancel.clone());
        let messages = vec![
            Message::system(teammate_system(&name, &role, approved)),
            Message::user(phase_prompt.clone()),
        ];
        let outcome = run_turn(&mut ctx, messages).await;

        if !approved {
            // 计划出炉：递交 lead 审批
            set_status(&state, &name, MemberStatus::AwaitingPlanApproval);
            let _ = append_inbox(&state.dir, "lead", &name, &format!("[plan for approval]\n{}", outcome.final_text));
            state.bus.publish(crate::core::event::Event::Notification(format!("teammate {name} submitted a plan for approval")));
        } else {
            // 本轮成果上报 lead
            if !outcome.final_text.is_empty() {
                let _ = append_inbox(&state.dir, "lead", &name, &outcome.final_text);
            }
            // teammate_idle hook：exit 非零 = 打回（反馈进 inbox， teammate 继续工作）
            if let Some(hooks) = &state.deps.hooks {
                if let Err(feedback) = hooks.run_named("teammate_idle", &name, &json!({ "agent": name, "result": outcome.final_text })).await {
                    let _ = append_inbox(&state.dir, &name, "hooks", &format!("keep working: {feedback}"));
                }
            }
            set_status(&state, &name, MemberStatus::Idle);
        }

        // idle：听 inbox 唤醒
        loop {
            notify.notified().await;
            if cancel.is_cancelled() {
                break;
            }
            let inbox = drain_inbox(&state.dir, &name);
            if inbox.is_empty() {
                continue;
            }
            // 审批结果修改 approved 状态
            for (from, text) in &inbox {
                if from == &"lead" && text.contains("Plan approved") {
                    approved = true;
                }
            }
            phase_prompt = inbox
                .iter()
                .map(|(from, text)| format!("[{from}] {text}"))
                .collect::<Vec<_>>()
                .join("\n");
            break;
        }
        if cancel.is_cancelled() {
            break;
        }
    }
    set_status(&state, &name, MemberStatus::Shutdown);
}

const READONLY_TEAM_TOOLS: &[&str] = &["read", "glob", "grep", "send_message", "team_task"];

fn build_ctx(state: &Arc<TeamState>, name: &str, _role: &str, model: &ModelRef, allowed: Option<&'static [&'static str]>, cancel: CancelToken) -> AgentContext {
    let agent_name = name.to_string();
    let session_id = state.session_id.clone();
    let session_id_event = session_id.clone();
    let bus = state.bus.clone();
    let agents = state.deps.agents.clone();
    let agent_name_tx = name.to_string();
    let session_id_tx = session_id.clone();
    AgentContext {
        registry: state.deps.registry.clone(),
        tracker: crate::tools::fs_tool::FileTracker::default(),
        workdir: state.deps.workdir.clone(),
        model: model.clone(),
        store: state.deps.store.clone(),
        max_turns: 16,
        mrm: Some(state.deps.mrm.clone()),
        allowed_tools: allowed,
        extras: Some(state.deps.extras.clone()),
        hooks: state.deps.hooks.clone(),
        loop_detector: crate::agent::loop_detect::LoopDetector::new(),
        cancel: Some(cancel),
        team: state.manager.upgrade(),
        team_identity: Some((session_id.clone(), agent_name.clone())),
        session_id: Some(session_id),
        agents: Some(state.deps.agents.clone()),
        bus: Some(state.bus.clone()),
        on_event: Arc::new(move |event| {
            let mut payload = match serde_json::to_value(&event) {
                Ok(v) => v,
                Err(_) => return,
            };
            if let Some(obj) = payload.as_object_mut() {
                obj.insert("agent".into(), json!(agent_name));
                obj.insert("session_id".into(), json!(session_id_event));
            }
            agents.push_transcript(&session_id_tx, &agent_name_tx, payload.clone());
            bus.publish(crate::core::event::Event::LlmDelta(payload));
        }),
    }
}

fn teammate_system(name: &str, role: &str, approved: bool) -> String {
    let mode = if approved {
        "You may use your full tool set to implement."
    } else {
        "You are in PLAN-ONLY mode: read-only tools. Produce a concrete plan and stop - the lead must approve it before you implement anything."
    };
    format!(
        "You are teammate \"{name}\" (role: {role}) in a kxen agent team. {mode} \
        Coordinate via send_message (to: \"lead\" or a teammate name) and team_task (claim/complete/list). \
        Report results to the lead when done, then go idle."
    )
}

fn set_status(state: &Arc<TeamState>, name: &str, status: MemberStatus) {
    if let Some(m) = lock(&state.members).iter_mut().find(|m| m.name == name) {
        m.status = status;
    }
    let activity_status = match status {
        MemberStatus::Working => crate::agent::activity::ActivityStatus::Working,
        MemberStatus::Idle => crate::agent::activity::ActivityStatus::Idle,
        MemberStatus::AwaitingPlanApproval => crate::agent::activity::ActivityStatus::Working,
        MemberStatus::Failed => crate::agent::activity::ActivityStatus::Failed,
        MemberStatus::Shutdown => crate::agent::activity::ActivityStatus::Shutdown,
    };
    state.deps.agents.set_status(&state.session_id, name, activity_status);
    let config = json!({ "session_id": state.session_id, "members": *lock(&state.members) });
    let _ = std::fs::write(state.dir.join("config.json"), serde_json::to_string_pretty(&config).unwrap_or_default());
    let label = match status {
        MemberStatus::Working => "working",
        MemberStatus::Idle => "idle",
        MemberStatus::AwaitingPlanApproval => "awaiting_plan_approval",
        MemberStatus::Failed => "failed",
        MemberStatus::Shutdown => "shutdown",
    };
    state.bus.publish(crate::core::event::Event::TaskUpdate { id: format!("team/{name}"), status: label });
}

pub(super) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
