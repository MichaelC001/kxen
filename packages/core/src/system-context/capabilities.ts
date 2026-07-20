import { Effect, Layer, Schema } from "effect"
import { makeLocationNode } from "../effect/app-node"
import { Location } from "../location"
import { SystemContext } from "./index"
import { SystemContextRegistry } from "./registry"

// kxen 能力说明：让模型知道 workflow / 角色 subagent / goal / safety 的存在与用法。
// 只陈述机制边界，不含内容级风控。
const TEXT = `kxen capabilities (this harness extends the base tool set):

- workflow tool: run a JavaScript script that orchestrates subagents. Globals inside the script: agent(prompt, {role?, label?}) -> {text}; pipeline(items, fn, {concurrency}) -> results; constraints() -> current resource limits; phase(name); args. The script holds loops, branching, and intermediate results itself; only the returned value comes back to you. Use it when a task needs fan-out (audit/migrate many files), multi-angle research with cross-checking, or fix-until-pass loops. Trigger: when the user says "workflow" / "ultracode" or the task is clearly larger than one agent's context.
- role subagents (via task tool): kxen-thinking (deep analysis, read-only), kxen-planning (task decomposition), kxen-execution (fast execution), kxen-review (adversarial review), kxen-research (external research). Each runs on a different provider/model chosen for the role; dispatch the role that fits the subtask, not the same model for everything.
- goal API: durable goals with lifecycle (draft/active/paused/complete/blocked/budget_limited) at /goal. Use for long-running multi-turn objectives with explicit completion criteria and budgets.
- safety: catastrophic operations (destroying the system, home directory, or .git) are hard-blocked at execution time and cannot be overridden by prompts. If a command is blocked, a rule id (F1-F5), reason, and suggestion are returned - pick an alternative path instead of retrying.
- .agents/ directory: project knowledge in OKF form. rules with alwaysApply are already injected above; everything else is listed in the index - read files on demand instead of expecting them in context.
`

const layer = Layer.effectDiscard(
  Effect.gen(function* () {
    const registry = yield* SystemContextRegistry.Service
    const context = SystemContext.make({
      key: SystemContext.Key.make("kxen/capabilities"),
      codec: Schema.toCodecJson(Schema.String),
      load: Effect.succeed(TEXT),
      baseline: (text) => text,
      update: (_previous, text) => text,
    })
    yield* registry.register({ key: SystemContext.Key.make("kxen/capabilities"), load: Effect.succeed(context) })
  }),
)

export const node = makeLocationNode({
  name: "kxen-capabilities-context",
  layer,
  deps: [Location.node, SystemContextRegistry.node],
})
