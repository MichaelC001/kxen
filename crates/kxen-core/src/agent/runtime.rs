//! Cross-domain execution policy values. Agent loop integration is provided by domain adapters.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionBudget {
    pub max_tokens: Option<u64>,
    pub max_turns: Option<u32>,
    pub max_wall_clock_ms: Option<u64>,
    pub max_tool_calls: Option<u32>,
    pub max_child_tasks: Option<u32>,
    pub max_delegation_depth: Option<u16>,
    pub max_message_hops: Option<u16>,
}

impl ExecutionBudget {
    pub fn most_restrictive(&self, other: &Self) -> Self {
        Self {
            max_tokens: minimum(self.max_tokens, other.max_tokens),
            max_turns: minimum(self.max_turns, other.max_turns),
            max_wall_clock_ms: minimum(self.max_wall_clock_ms, other.max_wall_clock_ms),
            max_tool_calls: minimum(self.max_tool_calls, other.max_tool_calls),
            max_child_tasks: minimum(self.max_child_tasks, other.max_child_tasks),
            max_delegation_depth: minimum(self.max_delegation_depth, other.max_delegation_depth),
            max_message_hops: minimum(self.max_message_hops, other.max_message_hops),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.max_turns == Some(0)
            || self.max_wall_clock_ms == Some(0)
            || self.max_tool_calls == Some(0)
            || self.max_child_tasks == Some(0)
            || self.max_message_hops == Some(0)
        {
            return Err("execution budget limits must be greater than zero".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BudgetUsage {
    pub tokens: u64,
    pub turns: u32,
    pub wall_clock_ms: u64,
    pub tool_calls: u32,
    pub child_tasks: u32,
    pub delegation_depth: u16,
    pub message_hops: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetExceeded {
    Tokens,
    Turns,
    WallClock,
    ToolCalls,
    ChildTasks,
    DelegationDepth,
    MessageHops,
}

impl ExecutionBudget {
    pub fn exceeded(&self, usage: BudgetUsage) -> Option<BudgetExceeded> {
        let checks = [
            self.max_tokens.is_some_and(|limit| usage.tokens > limit).then_some(BudgetExceeded::Tokens),
            self.max_turns.is_some_and(|limit| usage.turns > limit).then_some(BudgetExceeded::Turns),
            self.max_wall_clock_ms.is_some_and(|limit| usage.wall_clock_ms > limit).then_some(BudgetExceeded::WallClock),
            self.max_tool_calls.is_some_and(|limit| usage.tool_calls > limit).then_some(BudgetExceeded::ToolCalls),
            self.max_child_tasks.is_some_and(|limit| usage.child_tasks > limit).then_some(BudgetExceeded::ChildTasks),
            self.max_delegation_depth.is_some_and(|limit| usage.delegation_depth > limit).then_some(BudgetExceeded::DelegationDepth),
            self.max_message_hops.is_some_and(|limit| usage.message_hops > limit).then_some(BudgetExceeded::MessageHops),
        ];
        checks.into_iter().flatten().next()
    }
}

fn minimum<T: Ord + Copy>(left: Option<T>, right: Option<T>) -> Option<T> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_budget_uses_every_stricter_limit() {
        let platform = ExecutionBudget { max_tokens: Some(100), max_turns: Some(8), ..Default::default() };
        let bot = ExecutionBudget { max_tokens: Some(200), max_turns: Some(4), ..Default::default() };
        let effective = platform.most_restrictive(&bot);
        assert_eq!(effective.max_tokens, Some(100));
        assert_eq!(effective.max_turns, Some(4));
    }

    #[test]
    fn depth_and_hops_reject_only_after_limit() {
        let budget = ExecutionBudget { max_delegation_depth: Some(1), max_message_hops: Some(2), ..Default::default() };
        assert_eq!(budget.exceeded(BudgetUsage { delegation_depth: 1, message_hops: 2, ..Default::default() }), None);
        assert_eq!(
            budget.exceeded(BudgetUsage { delegation_depth: 2, message_hops: 2, ..Default::default() }),
            Some(BudgetExceeded::DelegationDepth)
        );
    }
}
