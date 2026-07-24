//! 跨 request 用量累加（P1-12）：一轮 tool loop 多次 LLM 请求，
//! 覆盖式只记末轮会漏算（状态栏 tokens 与 goal 预算入账的共同数据源）。

use super::context::AgentContext;

#[derive(Debug, Default)]
pub struct UsageAcc {
    input: u64,
    output: u64,
    /// 最近一次请求的 input（ctx 当前占用；累计值不代表窗口水位）
    last_input: u64,
    /// goal 已入账的累计值（增量入账的游标）
    charged: u64,
}

impl UsageAcc {
    pub fn push(&mut self, input: u64, output: u64) {
        self.input += input;
        self.output += output;
        self.last_input = input;
    }

    pub fn total(&self) -> (u64, u64) {
        (self.input, self.output)
    }

    pub fn last_input(&self) -> u64 {
        self.last_input
    }

    /// goal 预算入账增量：上次入账后新增的用量（无新 usage 返回 0，累计值不重复计）。
    pub fn goal_delta(&mut self) -> u64 {
        let now = self.input + self.output;
        let delta = now.saturating_sub(self.charged);
        self.charged = now;
        delta
    }
}

/// goal 记账：按 goal_delta 增量入账（累计值重复记会虚耗预算）。
/// 返回终态消息（BudgetLimited/Blocked）时调用方必须落终态文本并停。
pub(super) fn record_goal_turn(ctx: &mut AgentContext, acc: &mut UsageAcc, blocked_reason: Option<String>) -> Option<String> {
    // session 粒度：只推进本会话 goal，多会话并发不误伤
    let mut goal = crate::core::goal::Goal::focus_for(&crate::core::paths::goals_dir(), ctx.session_id.as_deref())?;
    let tokens = acc.goal_delta();
    if goal.record_turn(tokens, blocked_reason.as_deref(), false).is_err() {
        return None;
    }
    let _ = goal.save(&crate::core::paths::goals_dir());
    match goal.status {
        crate::core::goal::GoalStatus::BudgetLimited => {
            if let Some(bus) = &ctx.bus {
                bus.publish(crate::core::event::Event::GoalUpdate { id: goal.id.clone(), status: "budget_limited" });
            }
            Some("goal 预算耗尽（BudgetLimited），停止执行——调整预算后可 resume".to_string())
        }
        crate::core::goal::GoalStatus::Blocked => {
            if let Some(bus) = &ctx.bus {
                bus.publish(crate::core::event::Event::GoalUpdate { id: goal.id.clone(), status: "blocked" });
            }
            let reason = goal.block_reason.clone().unwrap_or_default();
            Some(format!("goal 连续阻塞已标记 Blocked：{reason}"))
        }
        _ => None,
    }
}

/// session 焦点 goal 的 wall 预算是否已超（P2-07 轮内检查点；仅 Active 才计费）。
pub(super) fn goal_wall_over(ctx: &AgentContext) -> bool {
    crate::core::goal::Goal::focus_for(&crate::core::paths::goals_dir(), ctx.session_id.as_deref())
        .is_some_and(|g| g.status == crate::core::goal::GoalStatus::Active && g.wall_exceeded())
}
