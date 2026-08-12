use crate::bot::run::{BotRunState, RunCommand, RunStatus, RunTriggerKind, UsageSummary};
use crate::core::identity::ResourceId;

use super::{BotExecutor, BotSystem};

impl BotExecutor {
    pub(super) fn finish(&self, run_id: &ResourceId, outcome: crate::agent::agent_loop::AgentOutcome) -> Result<BotRunState, String> {
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
        if current.cancellation_requested.is_some() {
            let terminal = self.finish_cancellation_with_usage(current, Some(usage))?;
            self.system.settle_run(&terminal, crate::core::shared::now_ms()).map_err(|error| error.to_string())?;
            return Ok(terminal);
        }
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
                crate::agent::agent_loop::AgentEvent::Done { .. } if !outcome.final_text.trim().is_empty() => {
                    let result = if current.spec.trigger.kind == RunTriggerKind::BuilderTest {
                        Ok(vec![crate::agent::dcp::ProviderNeutralPart::Text { text: outcome.final_text }])
                    } else {
                        run_definition(&self.system, &current)
                            .ok_or_else(|| "BotRun revision is unavailable".to_string())
                            .and_then(|definition| definition.output_parts(&outcome.final_text).map_err(|error| error.to_string()))
                    };
                    match result {
                        Ok(result) => RunCommand::Complete { result, usage, at_ms: now },
                        Err(message) => RunCommand::Fail { code: "output_contract_violation".into(), message, usage, at_ms: now },
                    }
                }
                _ => RunCommand::Fail { code: "agent_execution_failed".into(), message: outcome.final_text, usage, at_ms: now },
            }
        };
        let terminal = self.write(run_id, "terminal", command)?;
        self.system.settle_run(&terminal, now).map_err(|error| error.to_string())?;
        Ok(terminal)
    }
}

pub(super) fn run_definition(system: &BotSystem, run: &BotRunState) -> Option<crate::bot::BotDefinition> {
    system
        .definitions()
        .get(&run.spec.bot_id)
        .ok()?
        .revisions
        .values()
        .find(|revision| revision.revision_id == run.spec.revision_id)
        .map(|revision| revision.definition.clone())
}
