use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::agent::agent_loop::SessionExtrasRegistry;

use super::runner_support::{
    DcpAutoApprove, ensure_private_dir, filtered_child_environment, harden_tool_process_isolation, load_auth, load_runtime_policy,
};
use super::{DcpRunStatus, DcpRuntimePolicy, DcpStore};

const DEFAULT_CAPABILITIES: &[&str] = &[
    "read",
    "glob",
    "grep",
    "edit",
    "write",
    "delete",
    "exec",
    "task",
    "lsp",
    "webfetch",
    "websearch",
    "tool_search",
    "todo",
    "skill",
    "knowledge",
    "worktree",
    // workflow 进 catalog 但默认被 policy 拦下（allow_code_orchestration 特例，
    // 同 allow_shell 对 exec/task）：definition 可申请，批准权在 runtime policy
    "workflow",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DcpEventFormat {
    Jsonl,
    Text,
}

#[derive(Clone, Debug)]
pub struct DcpRuntimeOptions {
    pub data_dir: PathBuf,
    pub config_file: PathBuf,
    pub auth_file: PathBuf,
    pub consume_auth_file: bool,
    pub policy_file: Option<PathBuf>,
    pub event_format: DcpEventFormat,
    pub allow_shell: bool,
    pub allow_mcp: bool,
    pub pass_env: Vec<String>,
}

#[derive(Clone)]
pub struct DcpRunRequest {
    pub session_id: Option<String>,
    pub task: Option<String>,
    pub agent_file: Option<PathBuf>,
    pub workspace: Option<PathBuf>,
    pub rebind_workspace: bool,
    pub cancel: Option<crate::agent::cancel::CancelToken>,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DcpRunResult {
    pub session_id: String,
    pub run_id: String,
    pub status: DcpRunStatus,
    pub final_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub turns: u32,
    pub model: Option<crate::llm::ModelRef>,
}

pub type DcpEventSink = Arc<dyn Fn(DcpRuntimeEvent) + Send + Sync>;

#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DcpRuntimeEvent {
    SessionCreated { session_id: String },
    SessionResumed { session_id: String },
    RunStarted { session_id: String, run_id: String },
    RunInputRequired { session_id: String, run_id: String, operation_ids: Vec<String> },
    Agent { session_id: String, run_id: String, event: serde_json::Value },
    RunFinished { result: DcpRunResult },
}

pub struct DcpRuntime {
    pub(super) options: DcpRuntimeOptions,
    pub(super) policy: DcpRuntimePolicy,
    pub(super) store: DcpStore,
    pub(super) auth_store: Arc<crate::auth::credential::AuthStore>,
    pub(super) runtimes: Arc<crate::workspace_runtime::WorkspaceRuntimeRegistry>,
    pub(super) registry: Arc<crate::tools::task::TaskRegistry>,
    pub(super) extras: Arc<SessionExtrasRegistry>,
    pub(super) agents: Arc<crate::agent::activity::AgentRegistry>,
    pub(super) bus: crate::core::event::EventBus,
    pub(super) usage: Arc<std::sync::Mutex<HashMap<String, crate::core::usage::SessionUsage>>>,
    pub(super) sink: DcpEventSink,
    #[cfg(test)]
    pub(super) stream_override: Option<crate::llm::StreamFn>,
}

impl DcpRuntime {
    pub fn new(options: DcpRuntimeOptions, sink: DcpEventSink) -> Result<Self, String> {
        ensure_private_dir(&options.data_dir)?;
        let sessions_dir = options.data_dir.join("sessions");
        ensure_private_dir(&sessions_dir)?;
        let config = crate::core::config::Config::load(&options.config_file, None).map_err(|error| error.to_string())?;
        let bootstrap_policy = load_runtime_policy(&options)?;
        let has_tool_subprocesses = bootstrap_policy.allow_shell || bootstrap_policy.allow_mcp;
        let tool_process_isolated = if has_tool_subprocesses { harden_tool_process_isolation()? } else { true };
        if has_tool_subprocesses && options.consume_auth_file && !tool_process_isolated {
            return Err("one-shot Provider credentials with shell or MCP subprocesses require Linux or macOS process isolation".into());
        }
        let auth_store = Arc::new(load_auth(&options.auth_file, options.consume_auth_file)?);
        if has_tool_subprocesses && !auth_store.is_empty() && !options.consume_auth_file {
            return Err(
                "DCP execution with shell or MCP subprocesses and Provider credentials requires an explicit --auth-file and --consume-auth-file"
                    .into(),
            );
        }
        let tool_home = options.data_dir.join("tool-home");
        ensure_private_dir(&tool_home)?;
        let child_environment = filtered_child_environment(&bootstrap_policy, &tool_home)?;
        let runtimes = Arc::new(if bootstrap_policy.allow_mcp {
            crate::workspace_runtime::WorkspaceRuntimeRegistry::with_config_and_mcp_auto(
                options.config_file.clone(),
                config,
                Arc::new(DcpAutoApprove::new(options.data_dir.join("mcp-launch-audit.jsonl"), "mcp_stdio_launch")),
                child_environment,
            )
        } else {
            crate::workspace_runtime::WorkspaceRuntimeRegistry::with_config(options.config_file.clone(), config)
        });
        let registry = Arc::new(crate::tools::task::TaskRegistry::with_sessions_dir(sessions_dir.clone()));
        let extras = Arc::new(SessionExtrasRegistry::default());
        let agents = Arc::new(crate::agent::activity::AgentRegistry::default());
        agents.set_agents_root(sessions_dir.clone());
        Ok(Self {
            options,
            policy: bootstrap_policy,
            store: DcpStore::new(sessions_dir),
            auth_store,
            runtimes,
            registry,
            extras,
            agents,
            bus: crate::core::event::EventBus::default(),
            usage: Arc::new(std::sync::Mutex::new(HashMap::new())),
            sink,
            #[cfg(test)]
            stream_override: None,
        })
    }

    pub fn store(&self) -> &DcpStore {
        &self.store
    }

    pub fn base_capabilities() -> std::collections::BTreeSet<String> {
        crate::agent::tools_spec::core_tools()
            .into_iter()
            .chain(crate::agent::tools_spec::deferred_tools())
            .map(|tool| tool.function.name.clone())
            .filter(|name| DEFAULT_CAPABILITIES.contains(&name.as_str()))
            .collect()
    }

    pub(super) fn capabilities_for(runtime: &crate::workspace_runtime::WorkspaceRuntime) -> std::collections::BTreeSet<String> {
        let mut capabilities = Self::base_capabilities();
        for tool in runtime.mcp().all_tools() {
            if let Ok(name) = crate::mcp::tools::provider_tool_name(&tool.server, &tool.name) {
                capabilities.insert(name);
            }
        }
        // 动态工具族进 catalog 但默认被 policy 拦下（allow_dynamic_tools 特例，同 allow_mcp）：
        // definition 可以 optional 预声明，批准权在 runtime policy
        capabilities.insert(crate::agent::dynamic::FAMILY.to_string());
        capabilities
    }

    pub(super) async fn workspace_runtime(
        &self,
        workspace: &std::path::Path,
        policy: &DcpRuntimePolicy,
    ) -> Result<Arc<crate::workspace_runtime::WorkspaceRuntime>, String> {
        if policy.allow_mcp { self.runtimes.ready(workspace).await } else { self.runtimes.runtime(workspace) }
    }

    #[cfg(test)]
    pub(super) fn with_stream_override(mut self, stream: crate::llm::StreamFn) -> Self {
        self.stream_override = Some(stream);
        self
    }

    pub(super) fn stream_override(&self) -> Option<crate::llm::StreamFn> {
        #[cfg(test)]
        {
            self.stream_override.clone()
        }
        #[cfg(not(test))]
        {
            None
        }
    }
}
