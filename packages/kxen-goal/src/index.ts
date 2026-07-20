// Goal 生命周期：状态机 + 预算 + 持久化。
// 语义对齐 Kimi /write-goal（research/05）与 kxen 扩展（design/03）：
// 完成契约必填、阻塞三次规则、预算耗尽进 budget_limited。

export type GoalStatus =
  | "draft"
  | "queued"
  | "active"
  | "paused"
  | "complete"
  | "blocked"
  | "budget_limited"
  | "canceled"

export interface GoalBudget {
  tokens?: number
  turns?: number
  wallClockMs?: number
}

export interface GoalContract {
  objective: string
  completionCriteria: string
  constraints?: string
  budget?: GoalBudget
}

export interface Goal {
  id: string
  contract: GoalContract
  status: GoalStatus
  createdAt: number
  updatedAt: number
  activatedAt?: number
  turnsUsed: number
  tokensUsed: number
  lastBlockReason?: string
  consecutiveBlocks: number
  blockReason?: string
  verificationEvidence?: string
}

export class GoalError extends Error {
  constructor(
    readonly code: "invalid_transition" | "contract_incomplete" | "budget_exhausted" | "not_found",
    message: string,
  ) {
    super(message)
    this.name = "GoalError"
  }
}

const TERMINAL: ReadonlySet<GoalStatus> = new Set(["complete", "blocked", "canceled"])
const TRANSITIONS: Record<GoalStatus, GoalStatus[]> = {
  draft: ["queued", "active", "canceled"],
  queued: ["active", "canceled"],
  active: ["paused", "complete", "blocked", "budget_limited", "canceled"],
  paused: ["active", "canceled"],
  complete: [],
  blocked: ["active", "canceled"],
  budget_limited: ["active", "canceled"],
  canceled: [],
}

function assertContract(c: GoalContract) {
  if (!c.objective?.trim()) throw new GoalError("contract_incomplete", "objective is required")
  if (!c.completionCriteria?.trim()) throw new GoalError("contract_incomplete", "completionCriteria is required")
}

export function create(contract: GoalContract, id: string): Goal {
  assertContract(contract)
  const now = Date.now()
  return {
    id,
    contract,
    status: "draft",
    createdAt: now,
    updatedAt: now,
    turnsUsed: 0,
    tokensUsed: 0,
    consecutiveBlocks: 0,
  }
}

function transit(goal: Goal, to: GoalStatus, patch: Partial<Goal> = {}): Goal {
  if (TERMINAL.has(goal.status) && goal.status !== "blocked" && goal.status !== "budget_limited") {
    throw new GoalError("invalid_transition", `goal is terminal (${goal.status})`)
  }
  if (!TRANSITIONS[goal.status].includes(to)) {
    throw new GoalError("invalid_transition", `${goal.status} -> ${to} not allowed`)
  }
  return { ...goal, ...patch, status: to, updatedAt: Date.now() }
}

export const activate = (g: Goal) => transit(g, "active", { activatedAt: g.activatedAt ?? Date.now() })
export const pause = (g: Goal) => transit(g, "paused")
export const resume = (g: Goal) => transit(g, "active")
export const cancel = (g: Goal) => transit(g, "canceled")

export function complete(g: Goal, evidence: string): Goal {
  if (!evidence?.trim()) throw new GoalError("contract_incomplete", "completion requires verification evidence")
  return transit(g, "complete", { verificationEvidence: evidence })
}

export interface TurnUsage {
  tokens?: number
  blockedReason?: string
  terminal?: boolean
}

/** 记录一轮推进；返回新 goal。阻塞三次规则与预算检查在此。 */
export function recordTurn(g: Goal, usage: TurnUsage): Goal {
  if (g.status !== "active") throw new GoalError("invalid_transition", `recordTurn requires active, got ${g.status}`)
  let next: Goal = {
    ...g,
    turnsUsed: g.turnsUsed + 1,
    tokensUsed: g.tokensUsed + (usage.tokens ?? 0),
    updatedAt: Date.now(),
  }

  // 预算：任一维度耗尽 -> budget_limited
  const b = g.contract.budget
  if (b) {
    if (b.turns !== undefined && next.turnsUsed >= b.turns) return transit(next, "budget_limited")
    if (b.tokens !== undefined && next.tokensUsed >= b.tokens) return transit(next, "budget_limited")
    if (b.wallClockMs !== undefined && next.activatedAt && Date.now() - next.activatedAt >= b.wallClockMs) {
      return transit(next, "budget_limited")
    }
  }

  // 阻塞：terminal 当轮 blocked；同一原因累计 3 次才允许 blocked
  if (usage.blockedReason) {
    const same = g.lastBlockReason === usage.blockedReason
    const consecutive = same ? g.consecutiveBlocks + 1 : 1
    next = { ...next, lastBlockReason: usage.blockedReason, consecutiveBlocks: consecutive }
    if (usage.terminal || consecutive >= 3) {
      return transit(next, "blocked", { blockReason: usage.blockedReason })
    }
    return next
  }
  return { ...next, consecutiveBlocks: 0, lastBlockReason: undefined }
}

// --- 持久化（JSON 文件，一个 goal 一个文件） ---

export async function save(dir: string, goal: Goal): Promise<void> {
  await Bun.write(Bun.file(`${dir}/${goal.id}.json`), JSON.stringify(goal, null, 2))
}

export async function load(dir: string, id: string): Promise<Goal> {
  const file = Bun.file(`${dir}/${id}.json`)
  if (!(await file.exists())) throw new GoalError("not_found", `goal ${id} not found`)
  return file.json()
}

export async function list(dir: string): Promise<Goal[]> {
  const glob = new Bun.Glob("*.json")
  const out: Goal[] = []
  try {
    for await (const rel of glob.scan({ cwd: dir, onlyFiles: true })) {
      out.push(await Bun.file(`${dir}/${rel}`).json())
    }
  } catch {
    return []
  }
  return out.sort((a, b) => b.updatedAt - a.updatedAt)
}

export async function remove(dir: string, id: string): Promise<void> {
  await Bun.file(`${dir}/${id}.json`).delete()
}
