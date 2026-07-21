import path from "path"
import { Effect, Layer, Schema } from "effect"
import { makeLocationNode } from "../effect/app-node"
import { Location } from "../location"
import { Global } from "../global"
import { SystemContext } from "./index"
import { SystemContextRegistry } from "./registry"
import * as Goal from "kxen/goal"

// active/paused/blocked goal 注入 system-context（对齐 kimi-code goalInjection：
// 每轮重扫，状态变化经 Context Snapshot 生成 mid-conversation 更新）。
// 无 goal 时渲染一句占位（首基线不允许 unavailable/空 baseline，见 initialize）。
const FOCUS: ReadonlyArray<Goal.GoalStatus> = ["active", "paused", "blocked", "budget_limited"]

function render(g: Goal.Goal): string {
  const lines = [
    `kxen goal (${g.status}): ${g.contract.objective}`,
    `completion criteria: ${g.contract.completionCriteria}`,
  ]
  if (g.contract.constraints) lines.push(`constraints: ${g.contract.constraints}`)
  if (g.contract.budget) lines.push(`budget: ${JSON.stringify(g.contract.budget)} (used: turns ${g.turnsUsed}, tokens ${g.tokensUsed})`)
  if (g.blockReason) lines.push(`blocked reason: ${g.blockReason} (consecutive: ${g.consecutiveBlocks})`)
  if (g.status === "complete") lines.push(`verification evidence: ${g.verificationEvidence ?? ""}`)
  return lines.join("\n")
}

const layer = Layer.effectDiscard(
  Effect.gen(function* () {
    const registry = yield* SystemContextRegistry.Service
    const dir = path.join(Global.Path.data, "goals")

    const load = Effect.promise(async () => {
      const goals = await Goal.list(dir).catch(() => [] as Goal.Goal[])
      const current = goals.find((g) => FOCUS.includes(g.status))
      return current ? render(current) : "No active goal."
    })

    const context = SystemContext.make({
      key: SystemContext.Key.make("kxen/goal"),
      codec: Schema.toCodecJson(Schema.String),
      load,
      baseline: (text) => text,
      update: (_previous, text) => text,
    })

    yield* registry.register({ key: SystemContext.Key.make("kxen/goal"), load: Effect.succeed(context) })
  }),
)

export const node = makeLocationNode({
  name: "kxen-goal-context",
  layer,
  deps: [Location.node, SystemContextRegistry.node],
})
