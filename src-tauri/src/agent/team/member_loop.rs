// ---------------- teammate 常驻 loop ----------------

use crate::agent::agent_loop::{AgentContext, run_turn};
use crate::agent::cancel::CancelToken;
use crate::core::shared::lock;
use crate::llm::{Message, ModelRef};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Notify;

use super::TeamState;
use super::inbox::append_inbox;
use super::member_wake::{
    CLAIM_NUDGE, IDLE_TIMEOUT, IdleWake, first_prompt, idle_wait, inbox_has_plan_approval, inbox_text, push_inbox_transcript,
    round_messages, strip_system,
};
use super::types::MemberStatus;

// 参数逐项都是独立生命周期句柄（state/cancel/notify 各属不同所有者），打包 struct 只换层皮
#[allow(clippy::too_many_arguments)]
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
    // P0-1 跨 wake 历史：run_turn 就地累积（assistant 调用 + 工具结果 + 末轮文本），wake 只 append 新 inbox；
    // system 不进历史（roster 每轮实时重建），装配时 prepend 新鲜副本
    let mut history: Vec<Message> = Vec::new();
    let mut first_round = true;
    // approved 初值由调用方给：spawn 按 !plan_approval，restore 按落盘记录（崩溃前已批的不重批）
    let mut approved = approved;
    loop {
        if cancel.is_cancelled() {
            break;
        }
        set_status(&state, &name, MemberStatus::Working);
        // 阶段 ctx：plan_approval 未批准前只读
        let allowed: Option<&'static [&'static str]> = if approved { None } else { Some(READONLY_TEAM_TOOLS) };
        // 凭证预防刷新：build_ctx 只克隆共享 store 快照，长过期 token 不先换新则当轮失败下轮才自愈
        refresh_store_credentials(&state, &model).await;
        let mut ctx = build_ctx(&state, &name, &role, &model, allowed, cancel.clone());
        if first_round {
            first_round = false;
            // 首轮从 brief 建起（restore 场景并入残存 inbox 与本人未完成 claim，P1-2）
            history.push(Message::user(first_prompt(&state, &name, &prompt)));
        }
        let mut messages = round_messages(teammate_system(&state, &name, &role, approved), &history);
        let outcome = run_turn(&mut ctx, &mut messages).await;
        history = strip_system(messages);

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

        // idle：听 inbox 唤醒（P1-3：5min 超时自醒；shutdown 经 cancel 即刻醒）
        match idle_wait(&state, &name, &notify, &cancel, IDLE_TIMEOUT, approved).await {
            IdleWake::Cancel => break,
            IdleWake::Nudge => history.push(Message::user(CLAIM_NUDGE)),
            IdleWake::Inbox(inbox) => {
                // 审批结果修改 approved 状态
                if inbox_has_plan_approval(&inbox) {
                    approved = true;
                }
                // P1-4：来信入 transcript（AgentFocusView 可见）
                push_inbox_transcript(&state, &name, &inbox);
                history.push(Message::user(inbox_text(&inbox)));
            }
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

/// 凭证预防刷新（主会话 llm_task 同款）：克隆出 store 刷（避免持锁跨 await），成功回写共享 store。
/// 只刷 ctx 快照不回写时，共享 store 里的旧 refresh 被吊销后每轮快照都是废票，当轮失败下轮才自愈。
pub(super) async fn refresh_store_credentials(state: &Arc<TeamState>, model: &ModelRef) {
    let mut store = lock(&state.deps.store).clone();
    if crate::auth::refresh::ensure_fresh(&mut store, &model.provider, model.account.as_deref()).await {
        write_back_credential(&state.deps.store, &model.provider, model.account.as_deref(), &store);
    }
}

/// 刷新成果回写共享 store：克隆刷新的新凭证只活在副本里，不回写下一轮快照又拿旧值。
pub(super) fn write_back_credential(
    store: &Arc<std::sync::Mutex<crate::auth::credential::AuthStore>>,
    provider: &str,
    account: Option<&str>,
    refreshed: &crate::auth::credential::AuthStore,
) {
    let key = account.map(|a| crate::auth::credential::account_id(provider, a)).unwrap_or_else(|| provider.to_string());
    if let Some(cred) = refreshed.get(&key).cloned() {
        lock(store).insert(key, cred);
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
        MemberStatus::AwaitingPlanApproval => crate::agent::activity::ActivityStatus::AwaitingPlanApproval,
        MemberStatus::Failed => crate::agent::activity::ActivityStatus::Failed,
        MemberStatus::Shutdown => crate::agent::activity::ActivityStatus::Shutdown,
    };
    state.deps.agents.set_status(&state.session_id, name, activity_status);
    super::types::persist_config(state);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::event::EventBus;
    use std::path::{Path, PathBuf};

    fn deps() -> super::super::types::SpawnDeps {
        super::super::types::SpawnDeps {
            registry: Arc::new(crate::tools::task::TaskRegistry::new()),
            fallback_workdir: Arc::from(Path::new("/tmp")),
            store: Arc::new(std::sync::Mutex::new(crate::auth::credential::AuthStore::default())),
            mrm: Arc::new(std::sync::RwLock::new(Arc::new(crate::llm::mrm::ModelResourceManager::new(
                crate::core::config::Config::default(),
            )))),
            hooks: None,
            extras: Arc::new(crate::agent::agent_loop::SessionExtrasRegistry::default()),
            agents: Arc::new(crate::agent::activity::AgentRegistry::default()),
            approvals: None,
            mcp: None,
            lsp: Arc::new(crate::agent::team::LspPool::default()),
        }
    }

    fn state(tag: &str) -> (Arc<TeamState>, PathBuf) {
        let dir = std::env::temp_dir().join(format!("kxen-loop-{tag}-{}", std::process::id()));
        let mgr = crate::agent::team::TeamManager::new(dir.clone(), deps(), EventBus::default(), dir.join("sessions"), None);
        (mgr.state_for("s1"), dir)
    }

    /// 凭证预防刷新回写：ensure_fresh 换新的凭证必须落回共享 store（含命名账号键），
    /// 否则下轮 build_ctx 快照又拿旧值。grant 路径不触网测不了，见 auth::refresh 测试。
    #[test]
    fn write_back_credential_updates_shared_store() {
        use crate::auth::credential::CredentialKind;
        let shared = Arc::new(std::sync::Mutex::new(crate::auth::credential::AuthStore::default()));
        lock(&shared)
            .insert("anthropic".into(), CredentialKind::Oauth { access: "old".into(), refresh: "r1".into(), expires: 1, account_id: None });
        let mut refreshed = crate::auth::credential::AuthStore::default();
        refreshed.insert(
            "anthropic".into(),
            CredentialKind::Oauth { access: "new".into(), refresh: "r2".into(), expires: u64::MAX, account_id: None },
        );
        write_back_credential(&shared, "anthropic", None, &refreshed);
        let cred = lock(&shared).get("anthropic").cloned().unwrap();
        assert!(matches!(cred, CredentialKind::Oauth { ref access, .. } if access == "new"), "回写必须换成新 access");
        let mut named = crate::auth::credential::AuthStore::default();
        named.insert(
            "openai:work".into(),
            CredentialKind::Oauth { access: "w2".into(), refresh: "r3".into(), expires: u64::MAX, account_id: None },
        );
        write_back_credential(&shared, "openai", Some("work"), &named);
        assert!(lock(&shared).contains_key("openai:work"), "命名账号必须写进 provider:account 键");
    }

    /// 无需刷新时共享 store 原地不动（两条早退路都不触网）：未过期 OAuth 跳过；空 refresh 不可刷。
    #[tokio::test]
    async fn refresh_store_credentials_noop_preserves_store() {
        use crate::auth::credential::CredentialKind;
        let (state, dir) = state("refresh-noop");
        lock(&state.deps.store).insert(
            "anthropic".into(),
            CredentialKind::Oauth { access: "a".into(), refresh: "r".into(), expires: now_ms() + 3_600_000, account_id: None },
        );
        refresh_store_credentials(&state, &ModelRef::new("anthropic", "m")).await;
        let cred = lock(&state.deps.store).get("anthropic").cloned().unwrap();
        assert!(matches!(cred, CredentialKind::Oauth { ref access, .. } if access == "a"), "未过期必须原样保留");
        lock(&state.deps.store)
            .insert("openai".into(), CredentialKind::Oauth { access: "a2".into(), refresh: String::new(), expires: 1, account_id: None });
        refresh_store_credentials(&state, &ModelRef::new("openai", "m")).await;
        let cred = lock(&state.deps.store).get("openai").cloned().unwrap();
        assert!(matches!(cred, CredentialKind::Oauth { ref access, .. } if access == "a2"), "空 refresh 不可刷必须原样保留");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// AwaitingPlanApproval 透传 ActivityStatus：前端据此亮「待批准」warn，压成 Working 会误显示「工作中」
    #[test]
    fn awaiting_plan_approval_maps_through_to_activity() {
        let (state, dir) = state("awaiting");
        lock(&state.members).push(crate::agent::team::Member {
            name: "w".into(),
            role: "execution".into(),
            model: ModelRef::new("p", "m"),
            status: MemberStatus::Idle,
            plan_approval: true,
            prompt: String::new(),
            approved: false,
        });
        state.deps.agents.register("s1", "w", crate::agent::activity::AgentKind::Teammate, &ModelRef::new("p", "m"));
        set_status(&state, "w", MemberStatus::AwaitingPlanApproval);
        let list = state.deps.agents.list("s1");
        assert!(matches!(list[0].status, crate::agent::activity::ActivityStatus::AwaitingPlanApproval));
        assert_eq!(serde_json::to_value(list[0].status).unwrap(), "awaiting_plan_approval", "前端契约为 snake_case");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
