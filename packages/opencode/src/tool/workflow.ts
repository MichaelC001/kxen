import { Effect, Schema } from "effect"
import * as Tool from "./tool"
import { Session } from "@/session/session"
import type { TaskPromptOps } from "./task"
import { SessionV1 } from "@kxen/core/v1/session"
import { WorkflowRuntime, type AgentExecutor, type SubagentResult } from "kxen/workflow"
import { EffectBridge } from "@/effect/bridge"
import { InstanceState } from "@/effect/instance-state"

export const Parameters = Schema.Struct({
  script: Schema.String.annotate({
    description:
      "JavaScript body executed with top-level await. Available globals: agent(prompt, opts?) spawns a subagent and resolves to {text}; pipeline(items, fn, {concurrency}) maps with bounded parallelism; constraints() returns current resource limits; phase(name) marks a stage; args is the optional input.",
  }),
  args: Schema.optional(Schema.Unknown).annotate({ description: "Optional structured input exposed as `args`." }),
})
export type Parameters = Schema.Schema.Type<typeof Parameters>

function extractText(result: SessionV1.WithParts): string {
  return result.parts
    .filter((p): p is SessionV1.TextPart => p.type === "text")
    .map((p) => p.text)
    .join("\n")
    .trim()
}

export const WorkflowTool = Tool.define(
  "workflow",
  Effect.gen(function* () {
    const sessions = yield* Session.Service

    return {
      description:
        "Run a dynamic workflow script that orchestrates subagents. The script holds the loop, branching, and intermediate results itself; only the returned value comes back. Use for fan-out audits, batch migrations, multi-angle research, or fix-until-pass loops.",
      parameters: Parameters,
      execute: (params: Parameters, ctx: Tool.Context) =>
        Effect.gen(function* () {
          const bridge = yield* EffectBridge.make()
          const instanceCtx = yield* InstanceState.context

          const ops = ctx.extra?.promptOps as TaskPromptOps | undefined
          if (!ops) throw new Error("WorkflowTool requires promptOps in ctx.extra")
          const executeAgent: AgentExecutor = (promptText, opts) =>
            bridge.promise(
              Effect.gen(function* () {
                const child = yield* sessions.create({
                  parentID: ctx.sessionID,
                  title: `workflow agent${opts.label ? ` (${opts.label})` : ""}`,
                  agent: opts.role ? `kxen-${opts.role}` : undefined,
                })
                const result = yield* ops.prompt({
                  sessionID: child.id,
                  agent: opts.role ? `kxen-${opts.role}` : undefined,
                  parts: [{ type: "text", text: promptText }],
                })
                return { text: extractText(result) } satisfies SubagentResult
              }),
            )

          const runtime = new WorkflowRuntime({ executeAgent, maxAgentCalls: 200 })
          const state = yield* Effect.promise(() => runtime.run(params.script, params.args))

          if (state.status === "failed") {
            throw new Error(`workflow ${state.id} failed after ${state.agentCalls} agent calls: ${state.error}`)
          }
          return {
            output: JSON.stringify(
              {
                id: state.id,
                status: state.status,
                agentCalls: state.agentCalls,
                phases: state.phases.map((p) => ({ name: p.name, agentCount: p.agentCount })),
                result: state.result,
              },
              null,
              2,
            ),
            title: `workflow ${state.id} (${state.agentCalls} agents)`,
            metadata: { directory: instanceCtx.directory },
          }
        }),
    }
  }),
)
