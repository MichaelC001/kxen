use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::agent::agent_loop::{AgentContext, AgentEvent, PersistTurn};
use crate::llm::Message;

use super::runner_support::{
    DcpAutoApprove, ensure_private_dir, filtered_child_environment, recover_known_outcomes, same_message_content, validate_agent_output,
    workspace_scope,
};
use super::runner_types::{DcpRunRequest, DcpRunResult, DcpRuntime, DcpRuntimeEvent};
use super::{
    DcpAgentDefinition, DcpRunState, DcpRunStatus, DcpRunToolJournal, DcpRuntimePolicy, DcpSessionState, WorkspaceBinding,
    build_agent_definition,
};

impl DcpRuntime {
    pub async fn run(&self, request: DcpRunRequest) -> Result<DcpRunResult, String> {
        let policy = self.policy.clone();
        let mut session = match request.session_id.as_deref() {
            Some(session_id) => {
                let mut session = self.store.load_session(session_id)?;
                let workspace_path = request.workspace.as_deref().unwrap_or_else(|| Path::new(&session.workspace.root));
                let verified = session.workspace.verify(workspace_path, request.rebind_workspace)?;
                if request.rebind_workspace && verified.root != session.workspace.root {
                    session.workspace = verified;
                    session.updated_at_ms = crate::core::shared::now_ms();
                    self.store.save_session(&session)?;
                }
                let mut core_session = crate::core::session::load_meta(self.store.sessions_dir(), session_id)
                    .map_err(|error| format!("load core Session during Workspace verification: {error}"))?;
                if core_session.directory != session.workspace.root {
                    core_session.directory = session.workspace.root.clone();
                    crate::core::session::save_meta(self.store.sessions_dir(), &core_session)
                        .map_err(|error| format!("persist core Session Workspace binding: {error}"))?;
                }
                (self.sink)(DcpRuntimeEvent::SessionResumed { session_id: session_id.into() });
                session
            }
            None => {
                let task = request.task.as_deref().ok_or("new DCP Session requires a task")?;
                let workspace_path = request.workspace.as_deref().unwrap_or_else(|| Path::new("."));
                let workspace = WorkspaceBinding::capture(workspace_path)?;
                let runtime = self.workspace_runtime(Path::new(&workspace.root), &policy).await?;
                let capabilities = Self::capabilities_for(&runtime);
                let builder_capabilities = policy.permitted_catalog(&capabilities);
                let definition = match request.agent_file.as_deref() {
                    Some(path) => DcpAgentDefinition::parse_yaml(
                        &std::fs::read_to_string(path).map_err(|error| format!("read DCPAgent {}: {error}", path.display()))?,
                    )?,
                    None => {
                        build_agent_definition(task, &builder_capabilities, &runtime.mrm(), &self.auth_store, request.cancel.as_ref())
                            .await?
                    }
                };
                let lock = policy.resolve_lock(definition, &capabilities)?;
                let session = self.store.create_session(workspace, lock)?;
                (self.sink)(DcpRuntimeEvent::SessionCreated { session_id: session.session_id.clone() });
                session
            }
        };

        let _session_lease = self.store.acquire_session(&session.session_id)?;
        if let Some(last_run_id) = session.run_ids.last().cloned() {
            let last = self.store.load_run(&session.session_id, &last_run_id)?;
            if last.status.is_terminal() && !last.settled {
                let result = self.settle_terminal_run(&mut session, last)?;
                if request.task.is_none() {
                    return Ok(result);
                }
            }
            let last = self.store.load_run(&session.session_id, &last_run_id)?;
            if !last.status.is_terminal() {
                if request.task.as_ref().is_some_and(|task| task != &last.input) {
                    return Err(format!("Session has unfinished DCPRun {last_run_id}; resume it without a new task first"));
                }
                return self.execute_run(&mut session, last, &policy, request.cancel).await;
            }
        }
        let task = request.task.ok_or("NO_PENDING_WORK: the previous DCPRun is terminal and no continuation task was supplied")?;
        let run = self.store.create_run(&mut session, task)?;
        self.execute_run(&mut session, run, &policy, request.cancel).await
    }

    async fn execute_run(
        &self,
        session: &mut DcpSessionState,
        mut run: DcpRunState,
        policy: &DcpRuntimePolicy,
        external_cancel: Option<crate::agent::cancel::CancelToken>,
    ) -> Result<DcpRunResult, String> {
        if run.agent_definition_hash != session.agent.definition_hash {
            return Err(format!("DCPRun {} is bound to a different DCPAgent revision", run.run_id));
        }
        let policy_hash = policy.content_hash()?;
        if policy_hash != session.agent.policy_hash {
            return Err(format!(
                "DCP runtime policy drift: Session is locked to {}, current policy is {}",
                session.agent.policy_hash.as_str(),
                policy_hash.as_str()
            ));
        }
        let workspace = PathBuf::from(&session.workspace.root);
        let runtime = self.workspace_runtime(&workspace, policy).await?;
        let available = Self::capabilities_for(&runtime);
        for capability in &session.agent.effective_capabilities {
            if !available.contains(capability)
                || policy.denied_capabilities.contains(capability)
                || policy.allowed_capabilities.as_ref().is_some_and(|allowed| !allowed.contains(capability))
                || (!policy.allow_shell && matches!(capability.as_str(), "exec" | "task"))
                || (!policy.allow_mcp && capability.starts_with("mcp__"))
            {
                return Err(format!("locked DCPAgent capability is unavailable under the current runtime policy: {capability}"));
            }
        }
        let journal = Arc::new(DcpRunToolJournal::open(&self.store.run_dir(&session.session_id, &run.run_id)?)?);
        let stored_history = crate::core::session::load_history_checked(self.store.sessions_dir(), &session.session_id)
            .map_err(|error| format!("load DCP Session history: {error}"))?;
        let unknown = journal.reconcile(&stored_history)?;
        if !unknown.is_empty() {
            let operation_ids = unknown.into_iter().map(|operation| operation.operation_id).collect();
            return self.require_input_for_unknown(session, run, operation_ids);
        }
        recover_known_outcomes(self.store.sessions_dir(), &run, &journal)?;
        let stored_history = crate::core::session::load_history_checked(self.store.sessions_dir(), &session.session_id)
            .map_err(|error| format!("reload DCP Session history: {error}"))?;
        if !stored_history.iter().any(|message| message.id == run.input_message_id) {
            let mut message = crate::core::session::new_message(
                &session.session_id,
                crate::core::session::Role::User,
                vec![crate::core::session::Part::Text { text: run.input.clone().into() }],
            );
            message.id = run.input_message_id.clone();
            crate::core::session::append_message_idempotent_durable(self.store.sessions_dir(), &message)
                .map_err(|error| format!("persist DCPRun input: {error}"))?;
        }
        let history = crate::core::session::load_history_checked(self.store.sessions_dir(), &session.session_id)
            .map_err(|error| format!("load DCP Session history: {error}"))?;
        let mut messages = crate::agent::compact::flatten_stored(&history);
        messages.insert(0, Message::system(session.agent.definition.system_block()));

        let resolved = runtime
            .mrm()
            .resolve(&session.agent.definition.spec.execution.model_role, &self.auth_store)
            .await
            .ok_or_else(|| format!("no available model for role {}", session.agent.definition.spec.execution.model_role))?;
        let model = resolved.account.map_or_else(
            || crate::llm::ModelRef::new(&resolved.provider, &resolved.model),
            |account| crate::llm::ModelRef::with_account(&resolved.provider, &resolved.model, account),
        );
        run.status = DcpRunStatus::Running;
        run.model = Some(model.clone());
        run.error = None;
        run.updated_at_ms = crate::core::shared::now_ms();
        self.store.save_run(&run)?;
        (self.sink)(DcpRuntimeEvent::RunStarted { session_id: session.session_id.clone(), run_id: run.run_id.clone() });

        let cancel = external_cancel.unwrap_or_default();
        let timeout = session.agent.definition.spec.execution.max_wall_clock_ms.map(|limit| {
            let timeout_cancel = cancel.clone();
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(limit)).await;
                timeout_cancel.cancel();
            })
        });
        let persist = self.turn_persister(&session.session_id, &run.run_id, model.clone(), journal.clone());
        let sink = self.sink.clone();
        let event_session_id = session.session_id.clone();
        let event_run_id = run.run_id.clone();
        let allowed_tools = session.agent.effective_capabilities.clone();
        let path_scope = workspace_scope(&workspace, &allowed_tools);
        let tool_home = self.options.data_dir.join("tool-home");
        ensure_private_dir(&tool_home)?;
        let mut context = AgentContext {
            registry: self.registry.clone(),
            tracker: crate::tools::fs_tool::FileTracker::default(),
            workdir: Arc::from(workspace.as_path()),
            child_env: Some(filtered_child_environment(policy, &tool_home)?),
            path_grants: Arc::new(Default::default()),
            path_scope: Some(Arc::new(path_scope)),
            model,
            store: self.auth_store.clone(),
            max_turns: session.agent.definition.spec.execution.max_turns,
            max_pure_retries: Some(session.agent.definition.spec.execution.max_pure_retries),
            mrm: Some(runtime.mrm()),
            allowed_tools: Some(allowed_tools),
            extras: Some(self.extras.extras_for(&session.session_id)),
            hooks: None,
            loop_detector: crate::agent::loop_detect::LoopDetector::new(),
            cancel: Some(cancel),
            team: None,
            team_identity: None,
            session_id: Some(session.session_id.clone()),
            exec_scope: Some(format!("dcp_run_{}", run.run_id)),
            bound_goal_id: None,
            goal_binding_frozen: true,
            agents: Some(self.agents.clone()),
            bus: Some(self.bus.clone()),
            approvals: None,
            kanban_auto: policy.allow_shell.then(|| {
                Arc::new(DcpAutoApprove::new(
                    self.store.run_dir(&session.session_id, &run.run_id).expect("validated DCPRun path").join("shell-audit.jsonl"),
                    "shell_command",
                )) as Arc<dyn crate::tools::auto_approve::AutoApprove>
            }),
            mcp: Some(runtime.mcp()),
            mcp_approval_prechecked: policy.allow_mcp,
            lsp: Some(runtime.lsp()),
            notify: None,
            persist_compaction: None,
            persist_turn: Some(persist),
            tool_journal: Some(journal.clone()),
            domain_tools: None,
            auxiliary_usage: Arc::default(),
            usage_reporter: Some(crate::agent::agent_loop::UsageReporter::new_unscoped_in(
                run.run_id.clone(),
                self.usage.clone(),
                self.bus.clone(),
                self.options.data_dir.join("usage-attempts"),
            )),
            stream_override: self.stream_override(),
            on_event: Arc::new(move |event| {
                let event = serde_json::to_value(event)
                    .unwrap_or_else(|_| serde_json::json!({ "kind": "error", "message": "event serialization failed" }));
                sink(DcpRuntimeEvent::Agent { session_id: event_session_id.clone(), run_id: event_run_id.clone(), event });
            }),
        };
        let outcome = crate::agent::agent_loop::run_turn(&mut context, &mut messages).await;
        if let Some(task) = timeout {
            task.abort();
        }
        let unknown_operation_ids = journal
            .snapshot()
            .operations
            .into_iter()
            .filter(|operation| operation.phase == super::DcpToolPhase::OutcomeUnknown)
            .map(|operation| operation.operation_id)
            .collect();
        self.finish_run(session, run, outcome, context.model, unknown_operation_ids)
    }

    pub(super) fn finish_run(
        &self,
        session: &mut DcpSessionState,
        mut run: DcpRunState,
        outcome: crate::agent::agent_loop::AgentOutcome,
        model: crate::llm::ModelRef,
        unknown_operation_ids: Vec<String>,
    ) -> Result<DcpRunResult, String> {
        let output_error = validate_agent_output(&session.agent.definition.spec.output, &outcome.final_text).err();
        run.turns = outcome.turns;
        run.final_text = outcome.final_text;
        run.model = outcome.provider_model.or(Some(model));
        if !unknown_operation_ids.is_empty() {
            return self.require_input_for_unknown(session, run, unknown_operation_ids);
        }
        run.status = if outcome.aborted {
            DcpRunStatus::Canceled
        } else if matches!(outcome.terminal, AgentEvent::Done { .. }) && output_error.is_none() {
            DcpRunStatus::Completed
        } else {
            DcpRunStatus::Failed
        };
        run.error = match run.status {
            DcpRunStatus::Failed => Some(output_error.unwrap_or_else(|| run.final_text.clone())),
            _ => None,
        };
        run.updated_at_ms = crate::core::shared::now_ms();
        self.store.save_run(&run)?;
        self.settle_terminal_run(session, run)
    }

    fn require_input_for_unknown(
        &self,
        session: &DcpSessionState,
        mut run: DcpRunState,
        operation_ids: Vec<String>,
    ) -> Result<DcpRunResult, String> {
        debug_assert!(!operation_ids.is_empty());
        run.status = DcpRunStatus::InputRequired;
        run.error = Some(format!("UNKNOWN tool outcomes require explicit resolution before resume: {}", operation_ids.join(", ")));
        run.updated_at_ms = crate::core::shared::now_ms();
        self.store.save_run(&run)?;
        (self.sink)(DcpRuntimeEvent::RunInputRequired {
            session_id: session.session_id.clone(),
            run_id: run.run_id.clone(),
            operation_ids,
        });
        Ok(DcpRunResult {
            session_id: session.session_id.clone(),
            run_id: run.run_id,
            status: run.status,
            final_text: run.final_text,
            error: run.error,
            turns: run.turns,
            model: run.model,
        })
    }

    fn settle_terminal_run(&self, session: &mut DcpSessionState, mut run: DcpRunState) -> Result<DcpRunResult, String> {
        if !run.status.is_terminal() {
            return Err(format!("cannot settle non-terminal DCPRun {}", run.run_id));
        }
        if !run.final_text.is_empty() {
            let mut message = crate::core::session::new_message(
                &session.session_id,
                crate::core::session::Role::Assistant,
                vec![crate::core::session::Part::Text { text: run.final_text.clone().into() }],
            );
            message.id = format!("{}_final", run.run_id);
            message.model = run.model.clone();
            let history = crate::core::session::load_history_checked(self.store.sessions_dir(), &session.session_id)
                .map_err(|error| format!("load DCP Session history while settling final output: {error}"))?;
            if let Some(existing) = history.iter().find(|existing| existing.id == message.id) {
                if !same_message_content(existing, &message)? {
                    return Err(format!("DCPRun final message id collision: {}", message.id));
                }
            } else {
                crate::core::session::append_message_idempotent_durable(self.store.sessions_dir(), &message)
                    .map_err(|error| format!("persist DCPRun final output: {error}"))?;
            }
        }
        run.settled = true;
        run.updated_at_ms = crate::core::shared::now_ms();
        self.store.save_run(&run)?;
        session.updated_at_ms = run.updated_at_ms;
        self.store.save_session(session)?;
        let result = DcpRunResult {
            session_id: session.session_id.clone(),
            run_id: run.run_id,
            status: run.status,
            final_text: run.final_text,
            error: run.error,
            turns: run.turns,
            model: run.model,
        };
        (self.sink)(DcpRuntimeEvent::RunFinished { result: result.clone() });
        Ok(result)
    }

    fn turn_persister(&self, session_id: &str, run_id: &str, model: crate::llm::ModelRef, journal: Arc<DcpRunToolJournal>) -> PersistTurn {
        let sessions_dir = self.store.sessions_dir().to_path_buf();
        let session_id = session_id.to_string();
        let run_id = run_id.to_string();
        Arc::new(move |turn, parts| {
            let mut message = crate::core::session::new_message(&session_id, crate::core::session::Role::Assistant, parts.clone());
            message.id = format!("{run_id}_turn_{turn}");
            message.model = Some(model.clone());
            crate::core::session::append_message_idempotent_durable(&sessions_dir, &message).map_err(|error| error.to_string())?;
            journal.settle_parts(&parts)
        })
    }
}
