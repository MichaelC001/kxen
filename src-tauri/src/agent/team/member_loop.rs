// ---------------- teammate 常驻 loop ----------------

use crate::agent::agent_loop::{AgentContext, run_turn};
use crate::agent::cancel::CancelToken;
use crate::core::shared::lock;
use crate::llm::{Message, ModelRef};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Notify;

use super::TeamState;
use super::inbox::{append_inbox, drain_inbox};
use super::types::MemberStatus;

pub(super) async fn teammate_loop(
    state: Arc<TeamState>,
    name: String,
    role: String,
    model: ModelRef,
    prompt: String,
    approved: bool,
    cancel: CancelToken,
    notify: Arc<Notify>,
) {
    // 原始任务简报常驻：loop 每轮 messages 重建（无跨轮历史），唤醒只带新消息会丢任务上下文
    let base_prompt = prompt;
    let mut wake_prompt: Option<String> = None;
    // approved 初值由调用方给：spawn 按 !plan_approval，restore 按落盘记录（崩溃前已批的不重批）
    let mut approved = approved;
    loop {
        if cancel.is_cancelled() {
            break;
        }
        set_status(&state, &name, MemberStatus::Working);
        // 阶段 ctx：plan_approval 未批准前只读
        let allowed: Option<&'static [&'static str]> = if approved { None } else { Some(READONLY_TEAM_TOOLS) };
        let mut ctx = build_ctx(&state, &name, &role, &model, allowed, cancel.clone());
        let user_content = match &wake_prompt {
            None => base_prompt.clone(),
            Some(wake) => format!("Original task:\n{base_prompt}\n\n---\nNew messages:\n{wake}"),
        };
        let messages = vec![Message::system(teammate_system(&state, &name, &role, approved)), Message::user(user_content)];
        let outcome = run_turn(&mut ctx, messages).await;

        if !approved {
            // 计划出炉：递交 lead 审批（经 manager.send：observer 抄送 + 前端通知）
            let text = format!("[plan for approval]\n{}", outcome.final_text);
            match state.manager.upgrade() {
                Some(mgr) => {
                    let _ = mgr.send(&state, &name, "lead", &text);
                }
                None => {
                    let _ = append_inbox(&state.dir, "lead", &name, &text);
                }
            }
            set_status(&state, &name, MemberStatus::AwaitingPlanApproval);
        } else {
            // 本轮成果上报 lead（经 manager.send：observer 抄送 + 前端通知）
            if !outcome.final_text.is_empty() {
                match state.manager.upgrade() {
                    Some(mgr) => {
                        let _ = mgr.send(&state, &name, "lead", &outcome.final_text);
                    }
                    None => {
                        let _ = append_inbox(&state.dir, "lead", &name, &outcome.final_text);
                    }
                }
            }
            // teammate_idle hook：exit 非零 = 打回（反馈进 inbox， teammate 继续工作）
            if let Some(hooks) = &state.deps.hooks {
                let appr = crate::tools::exec::ApprovalCtx::new(
                    state.deps.approvals.as_deref(),
                    Some(&state.bus),
                    Some(&cancel),
                    Some(&state.session_id),
                );
                if let Err(feedback) = hooks
                    .run_named_with_approval("teammate_idle", &name, &json!({ "agent": name, "result": outcome.final_text }), appr.as_ref())
                    .await
                {
                    idle_rejected(&state, &name, &feedback);
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
            wake_prompt = Some(inbox.iter().map(|(from, text)| format!("[{from}] {text}")).collect::<Vec<_>>().join("\n"));
            break;
        }
        if cancel.is_cancelled() {
            break;
        }
    }
    set_status(&state, &name, MemberStatus::Shutdown);
}

const READONLY_TEAM_TOOLS: &[&str] = &["read", "glob", "grep", "send_message", "team_task"];

/// idle hook 打回：反馈进 inbox 并唤醒（不唤醒则 teammate 沉睡到下一封外部来信，打回形同虚设）。
fn idle_rejected(state: &Arc<TeamState>, name: &str, feedback: &str) {
    let _ = append_inbox(&state.dir, name, "hooks", &format!("keep working: {feedback}"));
    if let Some(n) = lock(&state.notifies).get(name) {
        n.notify_one();
    }
}

fn build_ctx(
    state: &Arc<TeamState>,
    name: &str,
    _role: &str,
    model: &ModelRef,
    allowed: Option<&'static [&'static str]>,
    cancel: CancelToken,
) -> AgentContext {
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
        // member 绑其 team session 的目录，不随 workspace switch 漂移（旧 workspace 的活跃 member 继续干活）
        workdir: state.workdir.clone(),
        model: model.clone(),
        // 每轮取实时凭证快照：探测/刷新晚于 deps 构造，冻结副本会让派发报假「无可用模型」
        store: lock(&state.deps.store).clone(),
        max_turns: 16,
        mrm: Some(state.deps.mrm.read().expect("mrm").clone()),
        allowed_tools: allowed,
        // lead 与 teammates 同会话作用域：共享该 session 的 extras（todo/挂载工具互通）
        extras: Some(state.deps.extras.extras_for(&state.session_id)),
        hooks: state.deps.hooks.clone(),
        loop_detector: crate::agent::loop_detect::LoopDetector::new(),
        cancel: Some(cancel),
        team: state.manager.upgrade(),
        team_identity: Some((session_id.clone(), agent_name.clone())),
        session_id: Some(session_id),
        agents: Some(state.deps.agents.clone()),
        bus: Some(state.bus.clone()),
        approvals: state.deps.approvals.clone(),
        mcp: state.deps.mcp.clone(),
        lsp: Some(state.deps.lsp.for_workspace(&state.workdir)),
        // teammate 不开通知通道：background 派发只从主会话发起（teammate 走 send_message 回 lead）
        notify: None,
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

pub(super) fn teammate_system(state: &Arc<TeamState>, name: &str, role: &str, approved: bool) -> String {
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
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}
