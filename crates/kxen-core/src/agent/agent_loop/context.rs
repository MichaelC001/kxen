//! loop 上下文与会话级共享态。

use crate::llm::ModelRef;
use crate::tools::fs_tool::FileTracker;
use crate::tools::task::TaskRegistry;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::events::AgentEvent;

pub type PersistCompaction = Arc<dyn Fn(&str, &[crate::llm::Message]) -> Result<(), String> + Send + Sync>;
/// 迭代级持久化回调：run loop 每完成一个 tool 迭代（下一次 LLM 请求前）调用，
/// 入参为迭代序号（turn）与本迭代 parts（Text? + ToolCall×N，output 已填）。
/// 失败必须 fail-closed 终止 run，不得静默吞（durable 缺口的副作用无记录）。
pub type PersistTurn = Arc<dyn Fn(u32, Vec<crate::core::session::Part>) -> Result<(), String> + Send + Sync>;
pub use super::usage::UsageReporter;

#[derive(Clone, Debug, Default)]
pub struct ResourcePathScope {
    pub read: Vec<PathBuf>,
    pub write: Vec<PathBuf>,
    pub execute: Vec<PathBuf>,
}

/// 会话级共享态：tool_search 挂载的 deferred 工具 + todo 清单。
/// 按 session 隔离（SessionExtrasRegistry 惰性创建），同 session 的 lead/teammate/subagent 共享。
#[derive(Default)]
pub struct SessionExtras {
    pub extra_tools: std::sync::Mutex<std::collections::HashSet<String>>,
    pub todos: crate::tools::todo::TodoStore,
    /// 已装载 skill（"name\x1fargs" 键）：同 args 禁止重调。
    pub loaded_skills: std::sync::Mutex<std::collections::HashSet<String>>,
    /// skill -> skill 递归深度（cap 3）。
    pub skill_depth: std::sync::atomic::AtomicU32,
    /// browser 工具的 per-session 单实例槽（懒启动；delete 经 close_browser 关 Chrome）
    pub browser: crate::tools::browser::BrowserSlot,
}

/// 按 session 隔离的 extras 注册表：进程级单例会让 A 会话的 todo/挂载工具
/// 泄露到 B 会话（P0-14），故按 session_id 惰性创建各自实例。
#[derive(Default)]
pub struct SessionExtrasRegistry {
    inner: std::sync::Mutex<std::collections::HashMap<String, Arc<SessionExtras>>>,
}

impl SessionExtrasRegistry {
    pub fn extras_for(&self, session_id: &str) -> Arc<SessionExtras> {
        crate::core::shared::lock(&self.inner).entry(session_id.to_string()).or_default().clone()
    }

    /// 会话销毁时清状态：下次取用重建空实例。
    pub fn drop_extras(&self, session_id: &str) {
        crate::core::shared::lock(&self.inner).remove(session_id);
    }

    /// 会话销毁前关浏览器（driver Drop 的进程兜底是 kill_on_drop，这里走显式 close 更干净）。
    pub async fn close_browser(&self, session_id: &str) {
        let extras = crate::core::shared::lock(&self.inner).get(session_id).cloned();
        if let Some(extras) = extras {
            extras.browser.close().await;
        }
    }
}

pub struct AgentContext {
    pub registry: Arc<TaskRegistry>,
    pub tracker: FileTracker,
    pub workdir: Arc<Path>,
    /// run 开始时捕获的原生选择器授权；凭证路径即使在集合中仍拒绝。
    pub path_grants: Arc<HashSet<PathBuf>>,
    /// None keeps Session semantics. Some closes resource access to the given
    /// roots and is mandatory for BotRun.
    pub path_scope: Option<Arc<ResourcePathScope>>,
    pub model: ModelRef,
    /// 运行开始时的凭证快照。子代理只克隆 Arc；仅 OAuth refresh 时通过
    /// `Arc::make_mut` 分离并更新当前 run 的快照。
    pub store: Arc<crate::auth::credential::AuthStore>,
    pub max_turns: u32,
    pub mrm: Option<Arc<crate::llm::mrm::ModelResourceManager>>,
    /// 子代理/kanban 列执行的工具白名单（None = 全部常驻工具）。custom DCP agent 的白名单来自
    /// 定义文件的 tools 字段（kanban::agents::resolve_allowed_tools 校验过的闭集）。
    pub allowed_tools: Option<Vec<String>>,
    pub extras: Option<Arc<SessionExtras>>,
    pub hooks: Option<Arc<crate::tools::hooks::HookRunner>>,
    pub loop_detector: crate::agent::loop_detect::LoopDetector,
    /// 取消令牌：loop 顶 / stream 消费 / 工具执行 三处检查点；子代理级联继承。
    pub cancel: Option<crate::agent::cancel::CancelToken>,
    /// lead 身份的 team 访问（None = 无 team 能力：subagent/workflow 子环境）。
    pub team: Option<Arc<crate::agent::team::TeamManager>>,
    /// teammate 身份（session_id, agent_name）：决定 send_message/team_task 可用。
    pub team_identity: Option<(String, String)>,
    /// lead 的 session id（team 工具路由用）。
    pub session_id: Option<String>,
    /// 无持久 session 的执行上下文（kanban run）给 exec/task 的 TaskOwner 作用域。
    /// session_id 必须保持 None：schedule/goal/browser 等 durable-session 门控工具继续 fail-closed。
    pub exec_scope: Option<String>,
    /// run 首次进入 Provider 前绑定的 Goal。结算必须按 id，不能在终态后重新 focus。
    pub bound_goal_id: Option<String>,
    /// `bound_goal_id = None` 既可能表示尚未捕获，也可能表示本 run 开始时没有 Goal。
    /// 该标记冻结后一律不得二次 focus，避免把早先请求的费用误扣到运行中才创建的 Goal。
    pub goal_binding_frozen: bool,
    /// 子代理活动注册表（teammate/subagent/workflow 统一视图）。
    pub agents: Option<Arc<crate::agent::activity::AgentRegistry>>,
    /// 事件总线（子代理流式事件上 UI 用）。
    pub bus: Option<crate::core::event::EventBus>,
    /// Ask 档审批 broker（exec 高危命令挂起等用户决定；None = 无审批通道按拒绝）。
    pub approvals: Option<Arc<crate::agent::approval::ApprovalBroker>>,
    /// 看板级自主授权句柄（P3）：仅 kanban 列执行挂载，命中 allowlist 的 Shell 自动放行。
    /// 交互会话/子代理一律 None——授权配置只能由人设置，Agent 侧只有消费路径。
    pub kanban_auto: Option<Arc<dyn crate::tools::auto_approve::AutoApprove>>,
    /// MCP 工具桥（mcp__server__tool 前缀调用；None = 未配置 MCP server）。
    pub mcp: Option<Arc<crate::mcp::McpManager>>,
    /// LSP 多语言诊断/导航（rust/ts/js/py/go per-language 懒启动；None = 未接线）。
    pub lsp: Option<Arc<crate::lsp::LspManager>>,
    /// 后台 agent 完成通知路由（仅主会话 ctx 开；子代理不再嵌套派发，None）。
    pub notify: Option<Arc<crate::agent::background::NotifyRouter>>,
    /// 主会话把 run 内 compaction 摘要落为 checkpoint；无持久化会话的子环境为 None。
    pub persist_compaction: Option<PersistCompaction>,
    /// 主会话把每个 tool 迭代落为一条 Assistant 消息；None = 纯内存（subagent/team/background，
    /// 行为与迭代持久化引入前完全一致）。
    pub persist_turn: Option<PersistTurn>,
    /// Durable tool intent/outcome boundary. BotRun mounts this adapter;
    /// ordinary Session agents keep it None and retain their current journal.
    pub tool_journal: Option<Arc<dyn crate::agent::dcp::ToolBoundaryJournal>>,
    /// Domain-specific tools use an injected port so the shared Agent loop does
    /// not depend on Bot, Kanban, Session or any other product aggregate.
    pub domain_tools: Option<Arc<dyn crate::agent::domain_tool::DomainToolRouter>>,
    /// completion judge 等辅助请求的 session/run 统计汇入点；Goal 已在调用处独立记账。
    pub auxiliary_usage: Arc<super::usage::AuxiliaryUsage>,
    /// 所有 lead/subagent/background/team run 共用的 session usage 汇入点。
    pub usage_reporter: Option<UsageReporter>,
    pub on_event: Arc<dyn Fn(AgentEvent) + Send + Sync>,
    /// 测试注入缝：替换 LLM 流式调用（None = LlmClient::stream_with_tools 静态分发）。
    /// 生产路径不设置；单测注入假流以直接覆盖 run 的重试/终态/预算分支。
    pub stream_override: Option<crate::llm::StreamFn>,
}

impl AgentContext {
    /// 执行侧白名单复验收口（execute_tool 用）：展示过滤只决定模型「看到什么」，
    /// 模型伪造/幻觉 tool_call 名可直接抵达 dispatch，必须在执行侧同口径复验。
    pub fn permits(&self, tool: &str) -> bool {
        super::helpers::tool_permitted(tool, self.allowed_tools.as_deref())
    }

    pub fn freeze_goal_binding(&mut self) -> Result<(), String> {
        if self.goal_binding_frozen {
            return Ok(());
        }
        self.bound_goal_id = crate::core::goal::Goal::focus_for_checked(&crate::core::paths::goals_dir(), self.session_id.as_deref())
            .map_err(|error| error.to_string())?
            .map(|goal| goal.id);
        self.goal_binding_frozen = true;
        Ok(())
    }
}
