//! BotRun adapter for the single shared Agent execution kernel.

mod context;
mod journal;
mod messages;
mod policy;
mod recovery;

pub use policy::workspace_id;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::agent::agent_loop::{AgentContext, PersistTurn, UsageReporter};
use crate::agent::cancel::CancelToken;
use crate::bot::run::{BotRunState, RunCommand, RunStatus, UsageSummary};
use crate::bot::system::BotSystem;
use crate::core::identity::{ActorRef, IdempotencyKey, ResourceId, SystemActor, TraceContext};

pub struct BotExecutorDeps {
    pub registry: Arc<crate::tools::task::TaskRegistry>,
    pub auth_store: Arc<std::sync::Mutex<Arc<crate::auth::credential::AuthStore>>>,
    pub runtimes: Arc<crate::workspace_runtime::WorkspaceRuntimeRegistry>,
    pub session_usage: Arc<std::sync::Mutex<std::collections::HashMap<String, crate::core::usage::SessionUsage>>>,
    pub bus: crate::core::event::EventBus,
}

pub struct BotExecutor {
    system: Arc<BotSystem>,
    deps: BotExecutorDeps,
    active: std::sync::Mutex<std::collections::HashMap<ResourceId, CancelToken>>,
}

struct AgentContextInput<'a> {
    run: &'a BotRunState,
    runtime: &'a Arc<crate::workspace_runtime::WorkspaceRuntime>,
    workdir: &'a Path,
    model: crate::llm::ModelRef,
    store: Arc<crate::auth::credential::AuthStore>,
    path_scope: crate::agent::agent_loop::ResourcePathScope,
    cancel: CancelToken,
}

impl BotExecutor {
    pub fn new(system: Arc<BotSystem>, deps: BotExecutorDeps) -> Self {
        Self { system, deps, active: std::sync::Mutex::new(Default::default()) }
    }

    pub fn cancel(&self, run_id: &ResourceId) -> bool {
        crate::core::shared::lock(&self.active).get(run_id).is_some_and(|cancel| {
            cancel.cancel();
            true
        })
    }

    pub async fn execute(&self, run_id: &ResourceId, active_workspace: &Path) -> Result<BotRunState, String> {
        let run = self.system.runs().get(run_id).map_err(|error| error.to_string())?;
        if run.status.is_terminal() || matches!(run.status, RunStatus::ApprovalRequired | RunStatus::InputRequired) {
            return Ok(run);
        }
        let cancel = CancelToken::new();
        if crate::core::shared::lock(&self.active).insert(run_id.clone(), cancel.clone()).is_some() {
            return Err(format!("BotRun is already active: {run_id}"));
        }
        let result = self.execute_reserved(run_id, active_workspace, cancel).await;
        crate::core::shared::lock(&self.active).remove(run_id);
        if let Err(error) = &result {
            self.persist_execution_error(run_id, error);
        }
        result
    }

    async fn execute_reserved(&self, run_id: &ResourceId, active_workspace: &Path, cancel: CancelToken) -> Result<BotRunState, String> {
        let mut run = self.system.runs().get(run_id).map_err(|error| error.to_string())?;
        if run.status == RunStatus::Queued {
            run = self.write(run_id, "start", RunCommand::Start { at_ms: crate::core::shared::now_ms() })?;
        }
        self.block_unknown_recovery(&run)?;
        let workspace = std::fs::canonicalize(active_workspace).map_err(|error| format!("active Workspace unavailable: {error}"))?;
        let sandbox = local_sandbox(run_id);
        let path_scope = policy::resolve_paths(&run.spec.permission.resources, &workspace, &sandbox)?;
        let execution_workdir = if run.spec.permission.resources.workspaces.is_empty() { sandbox } else { workspace.clone() };
        let runtime = self.deps.runtimes.runtime(&workspace)?;
        let store = crate::core::shared::lock(&self.deps.auth_store).clone();
        let resolved = runtime
            .mrm()
            .resolve(run.spec.mrm_role.as_str(), &store)
            .await
            .ok_or_else(|| format!("no available model for MRM role {}", run.spec.mrm_role))?;
        let model = resolved.account.map_or_else(
            || crate::llm::ModelRef::new(&resolved.provider, &resolved.model),
            |account| crate::llm::ModelRef::with_account(&resolved.provider, &resolved.model, account),
        );
        let frame = context::recorded(&run)?.map_or_else(|| context::compose(&self.system, &run), Ok)?;
        self.record_context(&run, &frame)?;
        run = self.system.runs().get(run_id).map_err(|error| error.to_string())?;
        let mut wire = messages::from_context(&frame);
        messages::append_history(&mut wire, &run.turns);
        messages::append_resume_state(&mut wire, &run);
        let timeout = run.spec.permission.budget.max_wall_clock_ms.map(|limit| {
            let cancel = cancel.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(limit)).await;
                cancel.cancel();
            })
        });
        let mut agent = self.agent_context(AgentContextInput {
            run: &run,
            runtime: &runtime,
            workdir: &execution_workdir,
            model,
            store,
            path_scope,
            cancel,
        });
        let outcome = crate::agent::agent_loop::run_turn(&mut agent, &mut wire).await;
        if let Some(timeout) = timeout {
            timeout.abort();
        }
        self.finish(run_id, outcome)
    }

    fn agent_context(&self, input: AgentContextInput<'_>) -> AgentContext {
        let AgentContextInput { run, runtime, workdir, model, store, path_scope, cancel } = input;
        let run_id = run.spec.run_id.clone();
        let persist_turn = self.persist_turn(run_id.clone());
        let bus = self.deps.bus.clone();
        let event_run_id = run_id.to_string();
        AgentContext {
            registry: self.deps.registry.clone(),
            tracker: crate::tools::fs_tool::FileTracker::default(),
            workdir: Arc::from(workdir),
            path_grants: Arc::new(Default::default()),
            path_scope: Some(Arc::new(path_scope)),
            model,
            store,
            max_turns: run.spec.permission.budget.max_turns.unwrap_or(32),
            mrm: Some(runtime.mrm()),
            allowed_tools: Some(run.spec.permission.capabilities.iter().map(ToString::to_string).collect()),
            extras: Some(Arc::default()),
            hooks: None,
            loop_detector: crate::agent::loop_detect::LoopDetector::new(),
            cancel: Some(cancel),
            team: None,
            team_identity: None,
            session_id: None,
            exec_scope: Some(format!("bot-run:{run_id}")),
            bound_goal_id: None,
            goal_binding_frozen: true,
            agents: None,
            bus: Some(bus.clone()),
            approvals: None,
            kanban_auto: None,
            mcp: None,
            lsp: Some(runtime.lsp()),
            notify: None,
            persist_compaction: None,
            persist_turn: Some(persist_turn),
            tool_journal: Some(Arc::new(journal::RunToolJournal::new(self.system.clone(), run_id.clone()))),
            domain_tools: Some(Arc::new(crate::bot::tools::BotToolRouter::new(self.system.clone(), run_id.clone()))),
            auxiliary_usage: Arc::default(),
            usage_reporter: Some(UsageReporter::new(format!("bot-run:{run_id}"), self.deps.session_usage.clone(), bus.clone())),
            stream_override: None,
            on_event: Arc::new(move |event| {
                if let Ok(payload) = serde_json::to_value(event) {
                    bus.publish(crate::core::event::Event::BotDelta { run_id: event_run_id.clone(), payload });
                }
            }),
        }
    }

    fn persist_turn(&self, run_id: ResourceId) -> PersistTurn {
        let system = self.system.clone();
        Arc::new(move |turn, parts| {
            let current = system.runs().get(&run_id).map_err(|error| error.to_string())?;
            let record_id = crate::bot::ids::deterministic_id("turn", &[run_id.as_str(), &turn.to_string()])?;
            let record = crate::agent::dcp::TurnRecord {
                record_id,
                kind: crate::agent::dcp::TurnRecordKind::Response,
                parts: messages::session_parts(&run_id, turn, parts)?,
                created_at_ms: crate::core::shared::now_ms(),
            };
            write_run(
                &system,
                &run_id,
                current.event_version,
                &format!("turn_{turn}"),
                RunCommand::RecordTurn { record, at_ms: crate::core::shared::now_ms() },
            )?;
            Ok(())
        })
    }

    fn record_context(&self, run: &BotRunState, frame: &crate::agent::dcp::ContextFrame) -> Result<(), String> {
        if run.turns.iter().any(|record| record.kind == crate::agent::dcp::TurnRecordKind::Request) {
            return Ok(());
        }
        let record = crate::agent::dcp::TurnRecord {
            record_id: crate::bot::ids::deterministic_id("turn", &[run.spec.run_id.as_str(), "context"])?,
            kind: crate::agent::dcp::TurnRecordKind::Request,
            parts: vec![crate::agent::dcp::ProviderNeutralPart::Data {
                schema_id: ResourceId::parse("dcp_context_frame")?,
                fields: std::collections::BTreeMap::from([
                    ("source_version".into(), frame.source_version.as_str().into()),
                    ("frame_json".into(), serde_json::to_string(frame).map_err(|error| error.to_string())?),
                ]),
            }],
            created_at_ms: crate::core::shared::now_ms(),
        };
        self.write(&run.spec.run_id, "context", RunCommand::RecordTurn { record, at_ms: crate::core::shared::now_ms() })?;
        Ok(())
    }

    fn finish(&self, run_id: &ResourceId, outcome: crate::agent::agent_loop::AgentOutcome) -> Result<BotRunState, String> {
        let current = self.system.runs().get(run_id).map_err(|error| error.to_string())?;
        if current.status != RunStatus::Running {
            return Ok(current);
        }
        let usage = outcome.stats.map_or_else(UsageSummary::default, |stats| UsageSummary {
            input_tokens: stats.input_tokens,
            output_tokens: stats.output_tokens,
            tool_calls: current.tool_operations.len() as u32,
            turns: outcome.turns,
            wall_clock_ms: stats.duration_ms,
        });
        let now = crate::core::shared::now_ms();
        let tokens = usage.input_tokens.saturating_add(usage.output_tokens);
        let token_budget_exceeded = current.spec.permission.budget.max_tokens.is_some_and(|limit| tokens > limit);
        let command = if token_budget_exceeded {
            RunCommand::Fail {
                code: "budget_exceeded".into(),
                message: format!("BotRun token budget exceeded: used {tokens}"),
                usage,
                at_ms: now,
            }
        } else if outcome.aborted {
            RunCommand::Cancel { reason: "BotRun canceled or wall-clock budget reached".into(), usage, at_ms: now }
        } else {
            match outcome.terminal {
                crate::agent::agent_loop::AgentEvent::Done { .. } if !outcome.final_text.trim().is_empty() => RunCommand::Complete {
                    result: vec![crate::agent::dcp::ProviderNeutralPart::Text { text: outcome.final_text }],
                    usage,
                    at_ms: now,
                },
                _ => RunCommand::Fail { code: "agent_execution_failed".into(), message: outcome.final_text, usage, at_ms: now },
            }
        };
        let terminal = self.write(run_id, "terminal", command)?;
        self.system.settle_run(&terminal, now).map_err(|error| error.to_string())?;
        Ok(terminal)
    }

    fn write(&self, run_id: &ResourceId, suffix: &str, command: RunCommand) -> Result<BotRunState, String> {
        let current = self.system.runs().get(run_id).map_err(|error| error.to_string())?;
        write_run(&self.system, run_id, current.event_version, suffix, command)
    }
}

fn write_run(
    system: &BotSystem,
    run_id: &ResourceId,
    expected_version: u64,
    suffix: &str,
    command: RunCommand,
) -> Result<BotRunState, String> {
    let key = crate::bot::ids::deterministic_id("idem", &[run_id.as_str(), suffix])?;
    system
        .runs()
        .execute(crate::bot::run::RunWrite {
            run_id: run_id.clone(),
            expected_version,
            idempotency_key: IdempotencyKey::parse(key.to_string())?,
            actor: ActorRef::System { actor: SystemActor::Runtime },
            trace: TraceContext::default(),
            command,
        })
        .map_err(|error| error.to_string())
}

fn local_sandbox(run_id: &ResourceId) -> PathBuf {
    std::env::temp_dir().join("kxen-bot-sandboxes").join(run_id.as_str())
}
