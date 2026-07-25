/** 子代理状态/kind 的统一展示映射：TopAgentBar chip、RightColumn 窗格卡、AgentFocusView 头三处共用，
 *  单点定义避免三处状态色漂移。 */
export const STATUS_TONE: Record<
  string,
  { tone: "ok" | "warn" | "accent" | "err" | "faint"; pulse: boolean }
> = {
  working: { tone: "ok", pulse: true },
  idle: { tone: "faint", pulse: false },
  // 计划待批准是在等人（lead/用户）动作：warn 脉冲提示，不显示成「工作中」
  awaiting_plan_approval: { tone: "warn", pulse: true },
  done: { tone: "accent", pulse: false },
  failed: { tone: "err", pulse: false },
  shutdown: { tone: "faint", pulse: false },
};

export const STATUS_TEXT: Record<string, string> = {
  working: "工作中",
  idle: "空闲",
  awaiting_plan_approval: "待批准",
  done: "已完成",
  failed: "失败",
  shutdown: "已关闭",
};

export const KIND_BADGE: Record<string, string> = {
  teammate: "team",
  subagent: "sub",
  workflow: "flow",
};
