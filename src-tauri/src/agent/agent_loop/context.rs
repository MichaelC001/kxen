//! loop 上下文与会话级共享态。

use crate::llm::ModelRef;
use crate::tools::fs_tool::FileTracker;
use crate::tools::task::TaskRegistry;
use std::path::Path;
use std::sync::Arc;

use super::events::AgentEvent;

/// 会话级共享态：tool_search 挂载的 deferred 工具 + todo 清单。
/// 放 AppState，跨 send_message 存续；子代理不继承（各自独立）。
#[derive(Default)]
pub struct SessionExtras {
    pub extra_tools: std::sync::Mutex<std::collections::HashSet<String>>,
    pub todos: crate::tools::todo::TodoStore,
    /// 已装载 skill（"name\x1fargs" 键）：同 args 禁止重调（调研 §2）。
    pub loaded_skills: std::sync::Mutex<std::collections::HashSet<String>>,
    /// skill -> skill 递归深度（cap 3）。
    pub skill_depth: std::sync::atomic::AtomicU32,
}

pub struct AgentContext {
    pub registry: Arc<TaskRegistry>,
    pub tracker: FileTracker,
    pub workdir: Arc<Path>,
    pub model: ModelRef,
    pub store: crate::auth::credential::AuthStore,
    pub max_turns: u32,
    pub mrm: Option<Arc<crate::llm::mrm::ModelResourceManager>>,
    /// 子代理工具白名单（None = 全部常驻工具）。
    pub allowed_tools: Option<&'static [&'static str]>,
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
    /// 子代理活动注册表（teammate/subagent/workflow 统一视图）。
    pub agents: Option<Arc<crate::agent::activity::AgentRegistry>>,
    /// 事件总线（子代理流式事件上 UI 用）。
    pub bus: Option<crate::core::event::EventBus>,
    /// Ask 档审批 broker（exec 高危命令挂起等用户决定；None = 无审批通道按拒绝）。
    pub approvals: Option<Arc<crate::agent::approval::ApprovalBroker>>,
    /// MCP 工具桥（mcp__server__tool 前缀调用；None = 未配置 MCP server）。
    pub mcp: Option<Arc<crate::mcp::McpManager>>,
    /// LSP 诊断（rust-analyzer 懒启动；None = 未接线）。
    pub lsp: Option<Arc<crate::lsp::LspManager>>,
    pub on_event: Arc<dyn Fn(AgentEvent) + Send + Sync>,
}
