//! 列执行 runtime（P2a；P4 起在卡专属 worktree 内执行，见 kanban/worktree.rs）：卡片进入 agent_run/workflow 列后的执行者，由 runner 调度（runner.rs）。
//!
//! 完成协议两阶段（套用 goal completion 四相语义——Claimed/Prepared/Scored/Unknown，类型不复用）：
//!   claim   —— run_started 事件经 Command durable，先于任何 LLM/脚本副作用（含 brief 落盘）；
//!   outcome —— run_finished/run_timeout 事件 durable，执行结束后落地。
//! 两个落点之间进程死亡 = Unknown：重启后 runner 按 orphan 提交 run_timeout 停车，
//! 绝不猜结果、绝不自动重发（对齐 completion.rs Prepared -> Unknown 的裁定语义）。
//! 结果判定不可猜：只有完整跑完且末轮文本带显式 VERDICT 行才落 success/failure；
//! 超时、中断、持久化失败一律 run_timeout（Unknown 处置），显式重试（runner 收养新 claim）才有第二次付费。
//! 列执行增量输出 = turns JSONL（persist_turn 回调逐迭代落盘），不依赖只广播不落盘的 llm.delta。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::agent::agent_loop::{PersistTurn, run_turn};
use crate::agent::cancel::CancelToken;
use crate::agent::subagent::SubagentDeps;
use crate::core::session as ses;
use crate::llm::{Message, ModelRef};

use super::context::{base_context, resolve_model};
use super::error::KanbanError;
use super::events::{EventKind, KanbanCommand, Outcome};
use super::land::{comment as land_comment, land_finished, land_timeout, publish_update, run_line};
use super::model::OnEnterKind;
use super::{Board, BoardAutoApprove, agents, render, store, worktree};

/// 默认列执行超时 30min：实现类列任务合法地长（编辑+构建+测试多轮工具调用），但 P1 租约语义
/// 要求绝不永远 running，必须有上限；workflow 引擎自身 10min 上限（workflow.rs）在此之下先触发。
pub const DEFAULT_RUN_TIMEOUT_MS: u64 = 30 * 60 * 1000;

/// 结果判定协议：成功/失败必须由 Agent 末轮文本显式声明，driver 不从正文内容猜结果。
const VERDICT_PROTOCOL: &str = "\n\nYou are executing one kanban column run. End your final message with exactly one verdict line: `VERDICT: success` or `VERDICT: failure`.";

/// 列执行依赖（廉价 Clone，runner 周期重建；对齐 SubagentDeps 的装配口径）。
#[derive(Clone)]
pub struct DriverDeps {
    pub registry: Arc<crate::tools::task::TaskRegistry>,
    pub workdir: Arc<Path>,
    pub store: crate::auth::credential::AuthStore,
    pub mrm: Arc<crate::llm::mrm::ModelResourceManager>,
    pub hooks: Option<Arc<crate::tools::hooks::HookRunner>>,
    pub bus: crate::core::event::EventBus,
    pub approvals: Option<Arc<crate::agent::approval::ApprovalBroker>>,
    /// workflow kind 派发子代理用（run_tool -> SubagentDeps 必填项）；agent_run 不注册活动。
    pub agents: Arc<crate::agent::activity::AgentRegistry>,
    pub mcp: Option<Arc<crate::mcp::McpManager>>,
    pub lsp: Option<Arc<crate::lsp::LspManager>>,
    /// 测试注入缝（同 AgentContext::stream_override）；生产为 None。
    pub stream_override: Option<crate::llm::StreamFn>,
    pub usage_reporter: Option<crate::agent::agent_loop::UsageReporter>,
}

#[derive(Debug)]
pub struct RunLanding {
    pub run_id: String,
    pub kind: LandingKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandingKind {
    Finished(Outcome),
    TimedOut,
}

#[derive(Debug)]
pub struct ExecuteFailure {
    /// 已 claim 的 run（None = 失败发生在 claim 之前，无副作用需要收口）。
    pub run_id: Option<String>,
    pub message: String,
}

fn fail(message: impl Into<String>) -> ExecuteFailure {
    ExecuteFailure { run_id: None, message: message.into() }
}
fn fail_at(run_id: &str, message: impl Into<String>) -> ExecuteFailure {
    ExecuteFailure { run_id: Some(run_id.to_string()), message: message.into() }
}

/// 执行步骤的失败分两类，决定 outcome 落点：
enum StepFailure {
    /// 确定性失败（定义/模型路由/脚本错误）：无副作用或副作用已闭合，落 run_finished(Failure)。
    Config(String),
    /// 结果不可知（持久化失败/中断）：落 run_timeout（Unknown 处置），显式重试才有第二次付费。
    Unknown(String),
}
/// 执行完毕的判定输入：verdict 来自末轮文本显式声明；None = 未声明（调用方注记后落 Failure）。
struct StepOutput {
    verdict: Option<Outcome>,
}

pub fn turns_path(workspace: &Path, board_id: &str, run_id: &str) -> Result<PathBuf, KanbanError> {
    Ok(store::board_dir(workspace, board_id)?.join("runs").join(format!("{run_id}.turns.jsonl")))
}

/// 从末轮文本解析显式 verdict：自尾向前取第一条声明（模型可能在前文引用 verdict 字样，以最后声明为准）。
pub fn parse_verdict(final_text: &str) -> Option<Outcome> {
    for line in final_text.lines().rev() {
        let line = line.trim();
        if line.eq_ignore_ascii_case("verdict: success") {
            return Some(Outcome::Success);
        }
        if line.eq_ignore_ascii_case("verdict: failure") {
            return Some(Outcome::Failure);
        }
    }
    None
}

/// 单次列执行：claim（或收养已有 claim）-> DCP 渲染 -> 执行 -> outcome 落地。
/// adopt = Some(run_id)：run 已被外部 Command claim（显式重试），driver 只执行与落地，不二次 claim。
pub async fn execute(
    workspace: &Path,
    board_id: &str,
    card_id: &str,
    deps: &DriverDeps,
    adopt: Option<String>,
) -> Result<RunLanding, ExecuteFailure> {
    let (run_id, column) = match adopt {
        Some(run_id) => {
            let board = Board::open(workspace, board_id).map_err(|e| fail(e.to_string()))?;
            let run = board
                .state()
                .runs
                .get(&run_id)
                .filter(|run| run.outcome.is_none())
                .ok_or_else(|| fail_at(&run_id, "adopted run is not open"))?;
            let column = board.state().column(&run.column_id).cloned().ok_or_else(|| fail_at(&run_id, "run column missing"))?;
            (run_id, column)
        }
        None => {
            let mut board = Board::open(workspace, board_id).map_err(|e| fail(e.to_string()))?;
            let card = board.state().cards.get(card_id).ok_or_else(|| fail(format!("card not found: {card_id}")))?;
            let column = board.state().column(&card.column_id).cloned().ok_or_else(|| fail("card column missing"))?;
            // 两阶段之 claim：run_started 先 durable，之后的所有失败都必须落到 outcome 事件
            let event = board.apply(KanbanCommand::RunStarted { card_id: card_id.to_string() }).map_err(|e| fail(e.to_string()))?;
            publish_update(&deps.bus, workspace, board_id);
            let EventKind::RunStarted(payload) = event.kind else { return Err(fail("run_started apply returned unexpected event")) };
            (payload.run_id, column)
        }
    };
    let events =
        store::load_events(&store::events_path(&store::board_dir(workspace, board_id).map_err(|e| fail_at(&run_id, e.to_string()))?))
            .map_err(|e| fail_at(&run_id, e.to_string()))?;
    // 渲染素材 = 含本次 run_started 的全量事件切片（attempt N 是上下文的一部分；同切片渲染确定）
    let prompt = render::render_card_context(&events, card_id).ok_or_else(|| fail_at(&run_id, "card vanished after claim"))?;
    let agent_name = column.on_enter.agent.clone().ok_or_else(|| fail_at(&run_id, "column has no agent reference"))?;
    let turns = turns_path(workspace, board_id, &run_id).map_err(|e| fail_at(&run_id, e.to_string()))?;
    let timeout_ms = column.timeout_ms.unwrap_or(DEFAULT_RUN_TIMEOUT_MS);
    let cancel = CancelToken::new();
    // 自主授权句柄无条件挂载：无 policy 时 AutoApproved 守卫拒绝、自然回落逐次审批（fail-closed 零特判）
    let auto = Arc::new(BoardAutoApprove {
        workspace: workspace.to_path_buf(),
        board_id: board_id.to_string(),
        run_id: run_id.clone(),
        bus: deps.bus.clone(),
    });
    // worktree 惰性分配：claim/adopt 之后、执行之前（WHY 见 kanban/worktree.rs 模块头）
    let workdir = worktree::allocate(workspace, board_id, card_id, &deps.bus).await;
    let body = async {
        // git 错误 = 确定性环境错误，走既有 Config 裁定（落 run_finished(Failure)）
        let workdir = workdir.map_err(StepFailure::Config)?;
        let scope = RunScope { workspace, board_id, run_id: &run_id, turns: &turns, auto: &auto, workdir };
        match column.on_enter.kind {
            OnEnterKind::AgentRun => run_agent(&scope, &agent_name, prompt, deps, cancel.clone()).await,
            OnEnterKind::Workflow => run_workflow(&scope, &agent_name, prompt, deps, cancel.clone()).await,
            // claim 守卫已拒绝不可执行列；此处是 adopt 路径的类型层兜底
            _ => Err(StepFailure::Config(format!("column kind {:?} is not executable", column.on_enter.kind))),
        }
    };
    let step = match tokio::time::timeout(Duration::from_millis(timeout_ms), body).await {
        Ok(step) => step,
        Err(_) => {
            cancel.cancel();
            comment(workspace, board_id, card_id, format!("run {run_id} timed out after {timeout_ms}ms"), &deps.bus);
            land_timeout(workspace, board_id, &run_id, &deps.bus).map_err(|e| fail_at(&run_id, e))?;
            return Ok(RunLanding { run_id, kind: LandingKind::TimedOut });
        }
    };
    let output = match step {
        Ok(output) => output,
        Err(StepFailure::Config(message)) => {
            comment(workspace, board_id, card_id, format!("run {run_id} failed: {message}"), &deps.bus);
            land_finished(workspace, board_id, &run_id, Outcome::Failure, &deps.bus).map_err(|e| fail_at(&run_id, e))?;
            return Ok(RunLanding { run_id, kind: LandingKind::Finished(Outcome::Failure) });
        }
        Err(StepFailure::Unknown(message)) => {
            let note = format!("run {run_id} outcome UNKNOWN: {message}; blocked pending explicit retry");
            comment(workspace, board_id, card_id, note, &deps.bus);
            land_timeout(workspace, board_id, &run_id, &deps.bus).map_err(|e| fail_at(&run_id, e))?;
            return Ok(RunLanding { run_id, kind: LandingKind::TimedOut });
        }
    };
    let outcome = match output.verdict {
        Some(outcome) => outcome,
        // 跑完未声明 VERDICT = 模型未完成协议动作，是显式的失败判定而非猜测
        None => {
            comment(workspace, board_id, card_id, format!("run {run_id} ended without a VERDICT line; landing failure"), &deps.bus);
            Outcome::Failure
        }
    };
    land_finished(workspace, board_id, &run_id, outcome, &deps.bus).map_err(|e| {
        // outcome 落不了（如目标列 WIP 满）：run 保持 open，进程内不重发（runner handled 集），
        // 重启后按 orphan -> Unknown 停车，等显式裁定
        comment(workspace, board_id, card_id, format!("run {run_id} outcome landing failed: {e}"), &deps.bus);
        fail_at(&run_id, e)
    })?;
    // 终态 detach（卡片落无出边列）：快照抢救产物后释放 worktree、保留分支；失败只注记不翻盘（WHY 见 worktree.rs）
    worktree::detach_if_terminal(workspace, board_id, card_id, &deps.bus).await;
    Ok(RunLanding { run_id, kind: LandingKind::Finished(outcome) })
}

/// 单次执行的作用域：位置参数打包，run_agent/run_workflow 共享。
struct RunScope<'a> {
    workspace: &'a Path,
    board_id: &'a str,
    run_id: &'a str,
    turns: &'a Path,
    auto: &'a Arc<BoardAutoApprove>,
    /// 本 run 的实际工作目录：卡专属 worktree；非 git workspace 降级为 workspace 根
    workdir: PathBuf,
}

async fn run_agent(
    scope: &RunScope<'_>,
    agent_name: &str,
    prompt: String,
    deps: &DriverDeps,
    cancel: CancelToken,
) -> Result<StepOutput, StepFailure> {
    let RunScope { workspace, board_id, run_id, turns, auto, .. } = *scope;
    let definition = agents::load(workspace, agent_name).map_err(|e| StepFailure::Config(format!("agent definition {agent_name}: {e}")))?;
    let model = resolve_model(&definition, deps).await.map_err(StepFailure::Config)?;
    let allowed =
        agents::profile_tools(&definition.permission_profile).ok_or_else(|| StepFailure::Config("unknown permission_profile".into()))?;
    let persist_failed = Arc::new(AtomicBool::new(false));
    // brief 先于任何 LLM 请求落盘：任务输入无记录则崩溃后无法审计（与 subagent 落 u 行同语义）
    run_line(
        turns,
        board_id,
        format!("{run_id}:u"),
        ses::Role::User,
        vec![ses::Part::Text { text: prompt.clone() }],
        None,
        &persist_failed,
    )
    .map_err(StepFailure::Unknown)?;
    let persist_turn: PersistTurn = {
        let (path, run, board, model, failed) =
            (turns.to_path_buf(), run_id.to_string(), board_id.to_string(), model.clone(), persist_failed.clone());
        Arc::new(move |turn: u32, parts: Vec<ses::Part>| {
            let mut message = ses::new_message(&board, ses::Role::Assistant, parts);
            message.id = format!("{run}:t{turn}");
            message.model = Some(model.clone());
            match ses::append_line_idempotent(&path, &message) {
                Ok(()) => Ok(()),
                Err(error) => {
                    failed.store(true, Ordering::Relaxed);
                    Err(error.to_string())
                }
            }
        })
    };
    let mut system =
        crate::agent::prompt::subagent_prompt(&definition.name, &definition.prompt, crate::core::config::coding_rules_enabled());
    system.push_str(VERDICT_PROTOCOL);
    let mut ctx = base_context(deps, model, allowed, Some(persist_turn), cancel, Some(auto.clone()));
    // 列执行在卡专属 worktree 内工作（tools 相对路径解析基准 = ctx.workdir）
    ctx.workdir = Arc::from(scope.workdir.as_path());
    ctx.exec_scope = Some(format!("kanban:{run_id}")); // exec/task 作用域；session_id 保持 None 使 durable-session 门控 fail-closed
    let mut messages = vec![Message::system(system), Message::user(prompt)];
    let outcome = run_turn(&mut ctx, &mut messages).await;
    if persist_failed.load(Ordering::Relaxed) {
        return Err(StepFailure::Unknown("turn persistence failed during run".into()));
    }
    if outcome.aborted {
        return Err(StepFailure::Unknown("run aborted; in-flight tool side effects indeterminate".into()));
    }
    // 末轮文本落盘：迭代已 persist_turn，final 缺档则恢复记录缺结论（与 subagent 同收口）
    if let Some(final_text) = crate::agent::agent_loop::new_final_text(&messages, &outcome) {
        run_line(
            turns,
            board_id,
            format!("{run_id}:final"),
            ses::Role::Assistant,
            vec![ses::Part::Text { text: final_text }],
            Some(ctx.model.clone()),
            &persist_failed,
        )
        .map_err(StepFailure::Unknown)?;
    }
    Ok(StepOutput { verdict: parse_verdict(&outcome.final_text) })
}

async fn run_workflow(
    scope: &RunScope<'_>,
    agent_name: &str,
    prompt: String,
    deps: &DriverDeps,
    cancel: CancelToken,
) -> Result<StepOutput, StepFailure> {
    let RunScope { workspace, board_id, run_id, turns, auto, .. } = *scope;
    // workflow 列复用 agent 定义文件：正文 = QuickJS 脚本（不发明第二套 workflow 存储）
    let script =
        agents::load(workspace, agent_name).map_err(|e| StepFailure::Config(format!("workflow definition {agent_name}: {e}")))?.prompt;
    let persist_failed = Arc::new(AtomicBool::new(false));
    // 卡片上下文落盘审计：workflow 不消费 LLM prompt，但触发输入必须可回放
    run_line(turns, board_id, format!("{run_id}:u"), ses::Role::User, vec![ses::Part::Text { text: prompt }], None, &persist_failed)
        .map_err(StepFailure::Unknown)?;
    let sub = SubagentDeps {
        registry: deps.registry.clone(),
        // 派发子代理同样落在卡专属 worktree
        workdir: Arc::from(scope.workdir.as_path()),
        path_grants: Arc::new(Default::default()),
        store: deps.store.clone(),
        mrm: deps.mrm.clone(),
        hooks: deps.hooks.clone(),
        extras: None,
        cancel: Some(cancel.clone()),
        agents: deps.agents.clone(),
        session_id: None,
        exec_scope: Some(format!("kanban:{run_id}")), // workflow 子代理与 agent_run 同作用域：exec 可用，session 门控不受影响
        bus: deps.bus.clone(),
        approvals: deps.approvals.clone(),
        mcp: deps.mcp.clone(),
        lsp: deps.lsp.clone(),
        stream_override: deps.stream_override.clone(),
        usage_reporter: deps.usage_reporter.clone(),
    };
    let mut ctx = base_context(deps, ModelRef::default(), None, None, cancel, Some(auto.clone()));
    ctx.workdir = Arc::from(scope.workdir.as_path());
    ctx.exec_scope = Some(format!("kanban:{run_id}")); // 同 run_agent：ctx 是 run_tool 的门控上下文
    // run_id = board:card:column:attempt（P1 派生）：同 run_id 重跑命中 journal 缓存不重复付费；
    // open_scoped 内部先哈希，run_id 含冒号不影响 journal 文件命名
    let text = crate::agent::workflow::run_tool(&script, sub, &ctx, Some(run_id)).await.map_err(StepFailure::Config)?;
    run_line(turns, board_id, format!("{run_id}:final"), ses::Role::Assistant, vec![ses::Part::Text { text }], None, &persist_failed)
        .map_err(StepFailure::Unknown)?;
    Ok(StepOutput { verdict: Some(Outcome::Success) })
}

/// 审计评论统一署名（runner 恢复用 kanban-runner 区分来源）。
fn comment(workspace: &Path, board_id: &str, card_id: &str, body: String, bus: &crate::core::event::EventBus) {
    land_comment(workspace, board_id, card_id, body, "kanban-driver", bus);
}

#[cfg(test)]
#[path = "driver/tests.rs"]
pub(crate) mod tests;
