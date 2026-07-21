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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamTaskStatus {
    Pending,
    InProgress,
    Completed,
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
    pub workdir: Arc<Path>,
    pub store: crate::auth::credential::AuthStore,
    pub mrm: Arc<ModelResourceManager>,
    pub hooks: Option<Arc<crate::tools::hooks::HookRunner>>,
    pub extras: Arc<crate::agent::agent_loop::SessionExtras>,
    pub agents: Arc<crate::agent::activity::AgentRegistry>,
}

pub(crate) struct TeamState {
    pub(crate) session_id: String,
    pub(crate) dir: PathBuf,
    pub(crate) manager: std::sync::Weak<TeamManager>,
    pub(crate) members: std::sync::Mutex<Vec<Member>>,
    pub(crate) cancels: std::sync::Mutex<HashMap<String, CancelToken>>,
    pub(crate) notifies: std::sync::Mutex<HashMap<String, Arc<Notify>>>,
    pub(crate) tasks: std::sync::Mutex<Vec<TeamTask>>,
    pub(crate) next_task_id: std::sync::atomic::AtomicU64,
    pub(crate) deps: SpawnDeps,
    pub(crate) bus: EventBus,
}
