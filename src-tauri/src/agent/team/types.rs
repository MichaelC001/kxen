// ---------------- 数据结构 ----------------

use crate::agent::cancel::CancelToken;
use crate::core::event::EventBus;
use crate::llm::mrm::ModelResourceManager;
use crate::llm::ModelRef;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Notify;

use super::manager::TeamManager;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberStatus {
    Working,
    Idle,
    AwaitingPlanApproval,
    Failed,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Member {
    pub name: String,
    pub role: String,
    pub model: ModelRef,
    pub status: MemberStatus,
    #[serde(default)]
    pub plan_approval: bool,
    /// 常驻任务简报：restore 重启 loop 的真相源（旧版落盘无此字段，空串降级 Shutdown）
    #[serde(default)]
    pub prompt: String,
    /// plan 审批是否已通过（restore 后 teammate_loop 的 approved 初值，避免重批）
    #[serde(default)]
    pub approved: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamTaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamTask {
    pub id: u64,
    pub title: String,
    pub status: TeamTaskStatus,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<u64>,
}

/// spawn 所需的共享依赖（构造 teammate ctx 用）。
#[derive(Clone)]
pub struct SpawnDeps {
    pub registry: Arc<crate::tools::task::TaskRegistry>,
    /// 只兜底：session metadata 缺失时的回退目录；真实 workdir 由 TeamManager::session_workdir 解析
    pub fallback_workdir: Arc<Path>,
    /// 共享句柄而非冻结副本：凭证探测/token 刷新晚于 TeamManager 构造，操作点 lock 取实时快照
    pub store: Arc<std::sync::Mutex<crate::auth::credential::AuthStore>>,
    pub mrm: std::sync::Arc<std::sync::RwLock<std::sync::Arc<ModelResourceManager>>>,
    pub hooks: Option<Arc<crate::tools::hooks::HookRunner>>,
    pub extras: Arc<crate::agent::agent_loop::SessionExtrasRegistry>,
    pub agents: Arc<crate::agent::activity::AgentRegistry>,
    pub approvals: Option<Arc<crate::agent::approval::ApprovalBroker>>,
    pub mcp: Option<Arc<crate::mcp::McpManager>>,
    /// team 自持 LSP 池：AppState 的 LspManager 随 workspace switch 重建，member 不许跟着漂移
    pub lsp: Arc<LspPool>,
}

/// per-workspace 懒建复用的 LspManager 池：member 诊断绑各自 team session 目录。
#[derive(Default)]
pub struct LspPool {
    pool: std::sync::Mutex<HashMap<PathBuf, Arc<crate::lsp::LspManager>>>,
}

impl LspPool {
    pub fn for_workspace(&self, root: &Path) -> Arc<crate::lsp::LspManager> {
        crate::core::shared::lock(&self.pool)
            .entry(root.to_path_buf())
            .or_insert_with(|| crate::lsp::LspManager::new(root.to_path_buf()))
            .clone()
    }
}

pub(crate) struct TeamState {
    pub(crate) session_id: String,
    pub(crate) dir: PathBuf,
    /// member 绑定的 team session 目录（建 state 时经 TeamManager::session_workdir 解析，此后不漂移）
    pub(crate) workdir: Arc<Path>,
    pub(crate) manager: std::sync::Weak<TeamManager>,
    pub(crate) members: std::sync::Mutex<Vec<Member>>,
    pub(crate) cancels: std::sync::Mutex<HashMap<String, CancelToken>>,
    pub(crate) notifies: std::sync::Mutex<HashMap<String, Arc<Notify>>>,
    pub(crate) tasks: std::sync::Mutex<Vec<TeamTask>>,
    pub(crate) next_task_id: std::sync::atomic::AtomicU64,
    pub(crate) deps: SpawnDeps,
    pub(crate) bus: EventBus,
}
