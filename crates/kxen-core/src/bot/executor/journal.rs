use std::sync::Arc;

use crate::agent::dcp::{ToolBoundaryAction, ToolBoundaryJournal};
use crate::bot::run::{ApprovalRequest, RunCommand, ToolExecutionResult, ToolIntent};
use crate::bot::system::BotSystem;
use crate::core::identity::{ActorRef, IdempotencyKey, ResourceId, SystemActor, TraceContext};
use crate::core::operation::{AttemptPhase, OperationOutcome};

pub(super) struct RunToolJournal {
    system: Arc<BotSystem>,
    run_id: ResourceId,
    calls: std::sync::Mutex<std::collections::HashMap<String, (ResourceId, ResourceId)>>,
}

impl RunToolJournal {
    pub fn new(system: Arc<BotSystem>, run_id: ResourceId) -> Self {
        Self { system, run_id, calls: std::sync::Mutex::new(Default::default()) }
    }

    fn ids(&self, tool_name: &str, arguments_json: &str) -> Result<(ResourceId, ResourceId, ResourceId), String> {
        let operation = crate::bot::ids::deterministic_id("op", &[self.run_id.as_str(), tool_name, arguments_json])?;
        let generation = crate::bot::ids::deterministic_id("gen", &[operation.as_str(), "1"])?;
        let call = crate::bot::ids::deterministic_id("call", &[operation.as_str()])?;
        Ok((operation, generation, call))
    }

    fn write(&self, suffix: &str, command: RunCommand) -> Result<crate::bot::run::BotRunState, String> {
        let current = self.system.runs().get(&self.run_id).map_err(|error| error.to_string())?;
        let key = crate::bot::ids::deterministic_id("idem", &[self.run_id.as_str(), suffix])?;
        self.system
            .runs()
            .execute(crate::bot::run::RunWrite {
                run_id: self.run_id.clone(),
                expected_version: current.event_version,
                idempotency_key: IdempotencyKey::parse(key.to_string())?,
                actor: ActorRef::System { actor: SystemActor::Runtime },
                trace: TraceContext::default(),
                command,
            })
            .map_err(|error| error.to_string())
    }

    fn needs_approval(&self, tool_name: &str) -> Result<bool, String> {
        let run = self.system.runs().get(&self.run_id).map_err(|error| error.to_string())?;
        let capability = ResourceId::parse(tool_name)?;
        let descriptor = self.system.capabilities().get(&capability).ok_or_else(|| format!("capability is not registered: {tool_name}"))?;
        Ok(match run.spec.permission.approval {
            crate::bot::ApprovalPolicy::AlwaysManual => true,
            crate::bot::ApprovalPolicy::ManualWhenRequired => descriptor.requires_approval,
            crate::bot::ApprovalPolicy::DenyControlledEffects => descriptor.requires_approval,
        })
    }
}

impl ToolBoundaryJournal for RunToolJournal {
    fn before(&self, _call_id: &str, tool_name: &str, arguments_json: &str, at_ms: u64) -> Result<ToolBoundaryAction, String> {
        let mut calls = crate::core::shared::lock(&self.calls);
        let (operation_id, generation, call_id) = self.ids(tool_name, arguments_json)?;
        calls.insert(_call_id.to_string(), (operation_id.clone(), generation.clone()));
        let mut run = self.system.runs().get(&self.run_id).map_err(|error| error.to_string())?;
        if let Some(attempt) = run.tool_operations.get(&operation_id).and_then(|operation| operation.attempt.as_ref()).cloned() {
            return match attempt.phase {
                AttemptPhase::Settled => replay(attempt.outcome.as_ref()),
                AttemptPhase::OutcomeKnown => {
                    self.write(
                        &format!("{}_settled", operation_id),
                        RunCommand::SettleTool { operation_id, generation: attempt.generation, at_ms },
                    )?;
                    replay(attempt.outcome.as_ref())
                }
                AttemptPhase::Prepared if run.approved_operations.contains(&operation_id) => {
                    self.write(&format!("{}_started", operation_id), RunCommand::MarkToolStarted { operation_id, generation, at_ms })?;
                    Ok(ToolBoundaryAction::Execute)
                }
                AttemptPhase::Prepared => Ok(ToolBoundaryAction::Pause { reason: "owner approval is pending".into() }),
                AttemptPhase::Started | AttemptPhase::OutcomeUnknown => {
                    self.write(
                        &format!("{}_unknown_recovery", operation_id),
                        RunCommand::MarkToolUnknown {
                            operation_id,
                            generation,
                            reason: "runtime recovered after tool start without a durable outcome".into(),
                            evidence: Vec::new(),
                            at_ms,
                        },
                    )?;
                    Err("tool outcome is UNKNOWN after recovery".into())
                }
                AttemptPhase::CanceledBeforeStart => Err("tool attempt was canceled before start".into()),
            };
        }
        if run.spec.permission.budget.max_tool_calls.is_some_and(|limit| run.tool_operations.len() >= limit as usize) {
            return Err("BotRun tool-call budget exhausted".into());
        }
        run = self.write(
            &format!("{}_prepared", operation_id),
            RunCommand::PrepareTool {
                operation_id: operation_id.clone(),
                generation: generation.clone(),
                intent: ToolIntent { call_id, capability_id: ResourceId::parse(tool_name)?, arguments_json: arguments_json.into() },
                at_ms,
            },
        )?;
        if self.needs_approval(tool_name)? {
            if run.spec.permission.approval == crate::bot::ApprovalPolicy::DenyControlledEffects {
                self.write(
                    &format!("{}_denied", operation_id),
                    RunCommand::Reject {
                        code: "controlled_effect_denied".into(),
                        message: format!("Bot policy denies controlled capability {tool_name}"),
                        at_ms,
                    },
                )?;
                return Ok(ToolBoundaryAction::Pause { reason: "controlled effect is denied".into() });
            }
            let approval_id = crate::bot::ids::deterministic_id("approval", &[self.run_id.as_str(), operation_id.as_str()])?;
            self.write(
                &format!("{}_approval", operation_id),
                RunCommand::RequestApproval {
                    request: ApprovalRequest {
                        approval_id,
                        operation_id,
                        summary: format!("Allow Bot capability {tool_name} with arguments {arguments_json}"),
                    },
                    at_ms,
                },
            )?;
            return Ok(ToolBoundaryAction::Pause { reason: "owner approval is required".into() });
        }
        self.write(&format!("{}_started", operation_id), RunCommand::MarkToolStarted { operation_id, generation, at_ms })?;
        Ok(ToolBoundaryAction::Execute)
    }

    fn after(&self, _call_id: &str, tool_name: &str, arguments_json: &str, output: &str, is_error: bool, at_ms: u64) -> Result<(), String> {
        let _guard = crate::core::shared::lock(&self.calls);
        let (operation_id, generation, _) = self.ids(tool_name, arguments_json)?;
        self.write(
            &format!("{}_outcome", operation_id),
            RunCommand::RecordToolOutcome {
                operation_id: operation_id.clone(),
                generation: generation.clone(),
                outcome: OperationOutcome::Succeeded { value: ToolExecutionResult { output: output.into(), is_error } },
                evidence: Vec::new(),
                at_ms,
            },
        )?;
        self.write(&format!("{}_settled", operation_id), RunCommand::SettleTool { operation_id, generation, at_ms })?;
        Ok(())
    }

    fn mark_unknown(&self, call_id: &str, reason: &str, at_ms: u64) -> Result<(), String> {
        let calls = crate::core::shared::lock(&self.calls);
        let (operation_id, generation) = calls.get(call_id).cloned().ok_or_else(|| "UNKNOWN tool operation is missing".to_string())?;
        self.write(
            &format!("{}_unknown", operation_id),
            RunCommand::MarkToolUnknown {
                operation_id: operation_id.clone(),
                generation,
                reason: reason.into(),
                evidence: Vec::new(),
                at_ms,
            },
        )?;
        self.system
            .recovery()
            .open(
                crate::core::identity::AggregateRef { kind: crate::core::identity::AggregateKind::BotRun, id: self.run_id.clone() },
                reason,
                vec![operation_id.to_string()],
                at_ms,
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn should_pause(&self) -> bool {
        self.system
            .runs()
            .get(&self.run_id)
            .is_ok_and(|run| matches!(run.status, crate::bot::run::RunStatus::ApprovalRequired | crate::bot::run::RunStatus::InputRequired))
    }
}

fn replay(outcome: Option<&OperationOutcome<ToolExecutionResult>>) -> Result<ToolBoundaryAction, String> {
    match outcome {
        Some(OperationOutcome::Succeeded { value }) => {
            Ok(ToolBoundaryAction::Replay { output: value.output.clone(), is_error: value.is_error })
        }
        Some(OperationOutcome::Failed { message, .. }) => Ok(ToolBoundaryAction::Replay { output: message.clone(), is_error: true }),
        None => Err("known tool outcome is missing its result".into()),
    }
}
