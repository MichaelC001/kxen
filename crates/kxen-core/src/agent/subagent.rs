//! 角色化 subagent：角色预设（model 经 mrm 路由 + 权限预设 + brief）+ 派发。
//! 角色 brief 全部英文（提示词规则），UI 文案不走这里。

use crate::agent::activity::AgentKind;
use crate::agent::agent_loop::{AgentContext, run_turn};
use crate::llm::Message;
use crate::llm::mrm::ModelResourceManager;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

/// 派发一个 subagent 所需的全部依赖：廉价 Clone，可跨并发派发安全共享。
#[derive(Clone)]
pub struct SubagentDeps {
    pub registry: Arc<crate::tools::task::TaskRegistry>,
    pub workdir: Arc<Path>,
    pub path_grants: Arc<std::collections::HashSet<std::path::PathBuf>>,
    pub store: Arc<crate::auth::credential::AuthStore>,
    pub mrm: Arc<ModelResourceManager>,
    pub hooks: Option<Arc<crate::tools::hooks::HookRunner>>,
    /// 父 session 的 extras（None = 无 session 上下文，dispatch 给临时实例）
    pub extras: Option<Arc<crate::agent::agent_loop::SessionExtras>>,
    pub cancel: Option<crate::agent::cancel::CancelToken>,
    pub agents: Arc<crate::agent::activity::AgentRegistry>,
    pub session_id: Option<String>,
    /// 父 ctx 的 exec 作用域（kanban run）：子代理继承后 exec/task 可用，session_id 门控不受影响。
    pub exec_scope: Option<String>,
    pub bus: crate::core::event::EventBus,
    pub approvals: Option<Arc<crate::agent::approval::ApprovalBroker>>,
    pub mcp: Option<Arc<crate::mcp::McpManager>>,
    pub lsp: Option<Arc<crate::lsp::LspManager>>,
    pub stream_override: Option<crate::llm::StreamFn>,
    pub usage_reporter: Option<crate::agent::agent_loop::UsageReporter>,
}

#[derive(Debug)]
pub struct DispatchResult {
    pub name: String,
    pub degraded_note: Option<String>,
    pub answer: String,
    /// run 最后一次真实尝试使用的模型和账号；账号可能在 retry 时轮转。
    pub model: crate::llm::ModelRef,
    pub degraded_from: Option<String>,
}

impl SubagentDeps {
    pub fn from_context(ctx: &AgentContext) -> Option<Self> {
        Some(Self {
            registry: ctx.registry.clone(),
            workdir: ctx.workdir.clone(),
            path_grants: ctx.path_grants.clone(),
            store: ctx.store.clone(),
            mrm: ctx.mrm.clone()?,
            hooks: ctx.hooks.clone(),
            extras: ctx.extras.clone(),
            cancel: ctx.cancel.clone(),
            agents: ctx.agents.clone()?,
            session_id: ctx.session_id.clone(),
            exec_scope: ctx.exec_scope.clone(),
            bus: ctx.bus.clone()?,
            approvals: ctx.approvals.clone(),
            mcp: ctx.mcp.clone(),
            lsp: ctx.lsp.clone(),
            stream_override: ctx.stream_override.clone(),
            usage_reporter: ctx.usage_reporter.clone(),
        })
    }
}

mod roles;
use roles::role_exists;
pub use roles::{PermissionProfile, RoleAgent, role_agent, role_agent_for};

/// agent 派发：角色 -> mrm 路由 model -> 独立子 loop -> (定名, 降级标注, 结果) 回传；
/// 定名给 background 拼完成通知，kind 统一进活动注册表供 UI 多窗格展示。
/// 降级标注 = mrm 状态注入：主绑定不可用（限流/满载）时给调用方一句可回执的说明，让编排模型看得见降级。
pub async fn dispatch(role: &str, prompt: String, deps: &SubagentDeps, kind: AgentKind) -> Result<DispatchResult, String> {
    // 未知 role 显式报错：静默回落只读会把实现类任务做成"跑完但没改"，比直接报错更难被发现
    if !role_exists(role, &deps.workdir) {
        return Err(format!(
            "unknown agent role '{role}' (builtin: thinking/planning/execution/review/research; custom: .agents/agents/<role>.md in a trusted project)"
        ));
    }
    let agent = role_agent_for(role, &deps.workdir);
    // 派发只选择模型；每次实际请求由 child context 重新做 admission、RPM 和并发占槽。
    let resolved = deps.mrm.resolve(role, &deps.store).await.ok_or_else(|| format!("no available model for role {role}"))?;

    let degraded_from = resolved.degraded_from;
    let model = match resolved.account {
        Some(account) => crate::llm::ModelRef::with_account(resolved.provider, resolved.model, account),
        None => crate::llm::ModelRef::new(resolved.provider, resolved.model),
    };
    let allowed: Vec<String> = agent.permission.allowed_tools().iter().map(|name| name.to_string()).collect();
    let session_id = deps.session_id.clone().unwrap_or_else(|| "default".into());
    // 定名 + 注册同一把锁内完成：并发派发同 role 不得同名并条（转录交错根因）
    let name = deps.agents.register_unique(&session_id, role, kind, &model);
    // 子代理独立取消句柄：agents.stop 按名停单个；父 run abort 经 watcher 级联（cancel.rs 的级联共识）。
    // watcher 随 dispatch 结束回收（done_tx drop 即唤醒退出分支），不留驻进程。
    let cancel = crate::agent::cancel::CancelToken::new();
    deps.agents.register_cancel(&session_id, &name, cancel.clone());
    let _cascade = deps.cancel.clone().map(|parent| {
        let child = cancel.clone();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            tokio::select! {
                _ = parent.wait() => child.cancel(),
                _ = done_rx => {}
            }
        });
        done_tx
    });

    // turn 级持久化（P1）：per-run JSONL（布局见 activity_disk），name 每 run 唯一即确定性 id。
    // 无 session 上下文（session_id=None，如 ping 派发）或未注入 agents_root 时保持纯内存。
    // subagent 一次性不续跑：落盘是恢复真源（registry 转录重建）与审计，崩溃窗口语义同主会话。
    let run_log = deps.session_id.as_ref().and_then(|_| deps.agents.run_log_path(&session_id, &name)).map(Arc::<Path>::from);
    let persist_turn: Option<crate::agent::agent_loop::PersistTurn> = run_log.as_ref().map(|path| {
        let path = path.clone();
        let session_id = session_id.clone();
        let member = name.clone();
        let model = model.clone();
        Arc::new(move |turn: u32, parts: Vec<crate::core::session::Part>| {
            run_line(&path, &session_id, format!("{member}:t{turn}"), crate::core::session::Role::Assistant, parts, Some(model.clone()))
        }) as crate::agent::agent_loop::PersistTurn
    });

    let mut child = AgentContext {
        registry: deps.registry.clone(),
        tracker: crate::tools::fs_tool::FileTracker::default(),
        workdir: deps.workdir.clone(),
        path_grants: deps.path_grants.clone(),
        path_scope: None,
        model: model.clone(),
        store: deps.store.clone(),
        max_turns: agent.max_turns,
        max_pure_retries: None,
        mrm: Some(deps.mrm.clone()),
        allowed_tools: if allowed.is_empty() { None } else { Some(allowed) },
        // 与父 run 同 session 共享 extras（todo/deferred 工具互通）；deps.extras 为 None（无 session 上下文）给临时实例
        extras: Some(deps.extras.clone().unwrap_or_default()),
        hooks: deps.hooks.clone(),
        cancel: Some(cancel),
        team: None,
        team_identity: None,
        session_id: Some(session_id.clone()),
        // 与 session_id 继承同理：kanban run 经 agent 工具派的子代理也要能 exec
        exec_scope: deps.exec_scope.clone(),
        bound_goal_id: None,
        goal_binding_frozen: false,
        agents: Some(deps.agents.clone()),
        bus: Some(deps.bus.clone()),
        approvals: deps.approvals.clone(),
        kanban_auto: None,
        mcp: deps.mcp.clone(),
        mcp_approval_prechecked: false,
        lsp: deps.lsp.clone(),
        notify: None, // 子代理不开通知通道：不嵌套派发（background 只从主会话发起）
        persist_compaction: None,
        persist_turn,
        tool_journal: None,
        domain_tools: None,
        auxiliary_usage: Arc::default(),
        usage_reporter: deps.usage_reporter.clone(),
        stream_override: deps.stream_override.clone(),
        loop_detector: crate::agent::loop_detect::LoopDetector::new(),
        on_event: {
            let bus = deps.bus.clone();
            let agents = deps.agents.clone();
            let name_event = name.clone();
            let sid = session_id.clone();
            // 无 session 上下文（"default" 兜底，如 ping 派发）：只广播不落 registry——
            // push_transcript 会写穿落盘，"default" 伪 session 不得产生磁盘垃圾
            let scoped = deps.session_id.is_some();
            Arc::new(move |event| {
                use serde_json::json;
                let mut payload = match serde_json::to_value(&event) {
                    Ok(v) => v,
                    Err(_) => return,
                };
                if let Some(obj) = payload.as_object_mut() {
                    obj.insert("agent".into(), json!(name_event));
                    obj.insert("session_id".into(), json!(sid));
                }
                if scoped {
                    agents.push_transcript(&sid, &name_event, payload.clone());
                }
                bus.publish(crate::core::event::Event::LlmDelta(payload));
            })
        },
    };

    let degraded_note = degraded_from.as_ref().map(|from| {
        format!("degraded: role '{from}' primary binding unavailable (rate limit or capacity); ran on {}/{}", model.provider, model.model)
    });
    let mut system_prompt = crate::agent::prompt::subagent_prompt(&agent.name, &agent.prompt, crate::core::config::coding_rules_enabled());
    // 子代理自知降级：产出质量受换型影响时应在最终报告里声明
    if let Some(note) = &degraded_note {
        write!(
            &mut system_prompt,
            "\n\n<scheduling>{note}. Flag this downgrade in your final report if it affects result quality.</scheduling>"
        )
        .expect("writing to String cannot fail");
    }
    let prompt = crate::core::shared::SharedText::from(prompt);
    // 先落盘后注入：brief 无记录则重启后的可检查记录缺少任务输入
    if let Some(path) = &run_log {
        run_line(
            path,
            &session_id,
            format!("{name}:u"),
            crate::core::session::Role::User,
            vec![crate::core::session::Part::Text { text: prompt.clone() }],
            None,
        )
        .map_err(|error| {
            deps.agents.set_status(&session_id, &name, crate::agent::activity::ActivityStatus::Failed);
            format!("run log persist failed: {error}")
        })?;
    }
    let mut messages = vec![Message::system(system_prompt), Message::user(prompt)];
    let outcome = run_turn(&mut child, &mut messages).await;
    // 末轮文本落盘：迭代已 persist_turn，final 缺档则恢复的记录缺结论（落盘内容与内存严格一致）
    if let Some(path) = &run_log
        && let Some(final_text) = crate::agent::agent_loop::new_final_text(&messages, &outcome)
    {
        run_line(
            path,
            &session_id,
            format!("{name}:final"),
            crate::core::session::Role::Assistant,
            vec![crate::core::session::Part::Text { text: final_text.into() }],
            Some(model.clone()),
        )
        .map_err(|error| format!("run log persist failed: {error}"))?;
    }
    deps.agents.set_status(
        &session_id,
        &name,
        if outcome.aborted { crate::agent::activity::ActivityStatus::Shutdown } else { crate::agent::activity::ActivityStatus::Done },
    );
    Ok(DispatchResult { name, degraded_note, answer: outcome.final_text, model: child.model, degraded_from })
}

/// per-run JSONL 追加一行（session Message 形态，与主会话/成员历史同基建）；
/// 失败如实上传（fail-closed：审计缺档不得静默继续）。
fn run_line(
    path: &Path,
    session_id: &str,
    id: String,
    role: crate::core::session::Role,
    parts: Vec<crate::core::session::Part>,
    model: Option<crate::llm::ModelRef>,
) -> Result<(), String> {
    let mut message = crate::core::session::new_message(session_id, role, parts);
    message.id = id;
    message.model = model;
    crate::core::session::append_line_idempotent(path, &message).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
