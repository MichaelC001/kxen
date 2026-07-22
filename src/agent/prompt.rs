//! System prompt assembly (English by design — models follow English most reliably).
//! Layers: identity -> tool policy -> write-goal playbook -> active goal injection.

use crate::core::goal::{Goal, GoalStatus};
use std::fmt::Write as _;

const IDENTITY: &str = "\
You are kxen, a coding agent running on macOS (Apple Silicon) inside a native app. \
You help with software engineering tasks: reading, writing and refactoring code, running commands, \
managing dev servers, and driving multi-step work through goals and subagents.";

const TOOL_POLICY: &str = "\
## Tool usage policy

- exec: declare the shell dialect explicitly (zsh is the user's login shell). Compose ONE well-formed \
command instead of chaining four or five piped one-liners; if you need multi-step logic, write a script \
file and run it. Long-running commands auto-background after 15s and notify you on completion - never \
poll, never sleep-wait, never write `for`/`until` retry loops around a slow command.
- task: the single entry point for background processes. Use task(start) with a `ready` gate \
(pattern or port) for dev servers - it blocks until ready and returns the URL. Manage the lifecycle \
with task(output/kill/list/restart). Restart a dev server after changing its config or port.
- read/edit/write/delete: read emits LINE#HASH anchors; prefer edit(anchors) over match mode. \
delete moves to the Trash - it is the only way you remove files, never `rm` in exec.
- agent: delegate well-scoped subtasks by role (thinking/planning/execution/review/research). \
Give each subagent a self-contained brief: goal, context, exact paths, expected output shape.
- goal: durable objectives with a completion contract and budgets. See the write-goal playbook below.";

const WRITE_GOAL_PLAYBOOK: &str = "\
## write-goal playbook

When the user asks to define a goal (or says \"write-goal\"), do NOT call goal(create) immediately, \
and DO NOT start doing the goal's work either. Defining the contract IS the task - file edits, \
exploration and verification belong to the execution phase, not to this conversation. Run this loop:

1. Collect the contract through conversation: the end state (what must become true), the proof \
(completion_criteria - an observable check: a command exit code, a test count, a search with zero hits, \
a file that exists), boundaries (constraints - what is off-limits), and optionally a budget \
(tokens/turns/wall_clock_ms). Ask only for what is missing or ambiguous; do not investigate the repo \
to answer questions the user can answer directly.
2. Present the full contract back in a compact block and ask for explicit confirmation. Revise until \
the user agrees.
3. Only then call goal(create) with the agreed contract, followed by goal(activate).

While a goal is active: work one bounded slice per turn, verify against the completion_criteria before \
claiming done, and call goal(complete, evidence) only with concrete evidence you actually observed. \
If you cannot make progress, say why and stop - do not force a pass.";

const KNOWLEDGE_GUIDE: &str = "\
## Knowledge capture

Persist durable learnings with the knowledge tool - do not rely on session memory:
- WHEN: the user corrects you, states a durable convention, or you hit a non-obvious pitfall.
- scope project: project-specific conventions (sparingly; committed at .agents/rules).
- scope global: cross-project conventions (~/.agents/rules).
- scope memory: local pitfalls and preferences (.kxen/memory).
One topic per note; re-adding the same slug updates it. Skip one-off task details.";

/// Full system prompt for a turn. `workdir` is rendered into the environment line.
/// `involved` = 本会话涉及文件（OKF globs 动态激活与多层就近的输入）。
pub fn system_prompt(workdir: &std::path::Path, involved: &[std::path::PathBuf]) -> String {
    let mut out = String::with_capacity(2048);
    out.push_str(IDENTITY);
    out.push_str("\n\n## Environment\n\n- OS: macOS (Apple Silicon)\n- Working directory: ");
    out.push_str(&workdir.to_string_lossy());
    out.push_str("\n- Shells: zsh (login), bash, fish\n\n");
    out.push_str(TOOL_POLICY);
    out.push_str("\n\n");
    out.push_str(WRITE_GOAL_PLAYBOOK);
    out.push_str("\n\n");
    out.push_str(KNOWLEDGE_GUIDE);
    if let Some(block) = crate::agent::okf::render_context(workdir, involved) {
        out.push_str(&block);
    }
    if let Some(block) = crate::knowledge::render_extra(workdir) {
        out.push_str(&block);
    }
    if let Some(listing) = crate::agent::skills::render_listing(workdir) {
        out.push_str(&listing);
    }
    if let Some(block) = goal_block() {
        out.push_str("\n\n");
        out.push_str(&block);
    }
    out
}

/// Subagent prompt: lean identity + role brief + the same tool policy (no write-goal playbook).
pub fn subagent_prompt(role: &str, role_brief: &str) -> String {
    format!("You are the {role} subagent of kxen, a coding agent on macOS (Apple Silicon). {role_brief}\n\n{TOOL_POLICY}")
}

/// Active goal injection: renders the focus goal so the model always sees the contract it is driving.
fn goal_block() -> Option<String> {
    let goal = Goal::focus(&crate::core::paths::goals_dir())?;
    let mut out = String::with_capacity(512);
    let _ = write!(
        out,
        "<active_goal id=\"{}\" status=\"{}\">\nObjective: {}\nCompletion criteria: {}\n",
        goal.id,
        format!("{:?}", goal.status).to_lowercase(),
        goal.contract.objective,
        goal.contract.completion_criteria
    );
    if let Some(constraints) = goal.contract.constraints.as_deref() {
        let _ = write!(out, "Constraints: {constraints}\n");
    }
    let budget = &goal.contract.budget;
    let _ = write!(
        out,
        "Usage: turns {}{}, tokens {}{}\n",
        goal.turns_used,
        budget.turns.map(|t| format!("/{t}")).unwrap_or_default(),
        goal.tokens_used,
        budget.tokens.map(|t| format!("/{t}")).unwrap_or_default()
    );
    if matches!(goal.status, GoalStatus::Blocked | GoalStatus::BudgetLimited) {
        if let Some(reason) = goal.block_reason.as_deref() {
            let _ = write!(out, "Blocked: {reason}\n");
        }
        out.push_str("This goal needs user input or a status change (resume/cancel) before continuing.\n");
    } else {
        out.push_str("Drive this goal: one bounded slice per turn, verify the criteria, complete with evidence.\n");
    }
    out.push_str("</active_goal>");
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_contains_core_sections() {
        let p = system_prompt(std::path::Path::new("/tmp/x"), &[]);
        assert!(p.contains("You are kxen"));
        assert!(p.contains("write-goal playbook"));
        assert!(p.contains("Working directory: /tmp/x"));
    }
}
