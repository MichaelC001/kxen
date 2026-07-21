import path from "path"
import { Effect, Schema } from "effect"
import * as Tool from "./tool"
import { Global } from "@kxen/core/global"
import * as Goal from "kxen/goal"

export const Parameters = Schema.Struct({
  action: Schema.Literals(["create", "get", "activate", "pause", "resume", "complete", "cancel", "list"]).annotate({
    description: "Lifecycle action to perform.",
  }),
  id: Schema.optional(Schema.String).annotate({ description: "Goal id (required for all actions except create/list)." }),
  contract: Schema.optional(
    Schema.Struct({
      objective: Schema.String,
      completionCriteria: Schema.String,
      constraints: Schema.optional(Schema.String),
      budget: Schema.optional(
        Schema.Struct({
          tokens: Schema.optional(Schema.Finite),
          turns: Schema.optional(Schema.Finite),
          wallClockMs: Schema.optional(Schema.Finite),
        }),
      ),
    }),
  ).annotate({ description: "Required for create: objective + completionCriteria (+ optional constraints/budget)." }),
  evidence: Schema.optional(Schema.String).annotate({ description: "Required for complete: verification evidence." }),
  blockedReason: Schema.optional(Schema.String).annotate({
    description: "For pause: reason. Same reason 3 turns in a row escalates to blocked (terminal blocks immediately).",
  }),
  terminal: Schema.optional(Schema.Boolean).annotate({ description: "Mark the block reason as terminal." }),
})
export type Parameters = Schema.Schema.Type<typeof Parameters>

const dir = () => path.join(Global.Path.data, "goals")

const show = (g: Goal.Goal) => ({
  id: g.id,
  status: g.status,
  objective: g.contract.objective,
  completionCriteria: g.contract.completionCriteria,
  budget: g.contract.budget,
  turnsUsed: g.turnsUsed,
  tokensUsed: g.tokensUsed,
  consecutiveBlocks: g.consecutiveBlocks,
  blockReason: g.blockReason,
  verificationEvidence: g.verificationEvidence,
})

export const GoalTool = Tool.define(
  "goal",
  Effect.succeed({
    description:
      "Manage durable goals: create with a completion contract (objective + completionCriteria), then drive the lifecycle (activate/pause/resume/complete/cancel). Goals persist across turns with budgets; use `list`/`get` to inspect current state before continuing work on one.",
    parameters: Parameters,
    execute: (params: Parameters, ctx: Tool.Context) =>
      Effect.gen(function* () {
        const requireId = () => {
          if (!params.id) throw new Error(`action ${params.action} requires id`)
          return params.id
        }
        switch (params.action) {
          case "create": {
            if (!params.contract) throw new Error("create requires contract (objective + completionCriteria)")
            const id = `goal_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`
            const g = Goal.create(params.contract, id)
            yield* Effect.promise(() => Goal.save(dir(), g)).pipe(Effect.orDie)
            return { title: `goal created: ${id}`, output: JSON.stringify(show(g), null, 2), metadata: {} }
          }
          case "list": {
            const goals = yield* Effect.promise(() => Goal.list(dir())).pipe(Effect.orDie)
            return {
              title: `${goals.length} goal(s)`,
              output: goals.length === 0 ? "no goals" : JSON.stringify(goals.map(show), null, 2),
              metadata: {},
            }
          }
          case "get": {
            const g = yield* Effect.promise(() => Goal.load(dir(), requireId())).pipe(Effect.orDie)
            return { title: `goal ${g.id}: ${g.status}`, output: JSON.stringify(show(g), null, 2), metadata: {} }
          }
          default: {
            const g = yield* Effect.promise(() => Goal.load(dir(), requireId())).pipe(Effect.orDie)
            const next =
              params.action === "activate"
                ? Goal.activate(g)
                : params.action === "pause"
                  ? params.blockedReason
                    ? Goal.recordTurn(g, { blockedReason: params.blockedReason, terminal: params.terminal })
                    : Goal.pause(g)
                  : params.action === "resume"
                    ? Goal.resume(g)
                    : params.action === "complete"
                      ? Goal.complete(g, params.evidence ?? "")
                      : Goal.cancel(g)
            yield* Effect.promise(() => Goal.save(dir(), next)).pipe(Effect.orDie)
            return { title: `goal ${next.id}: ${next.status}`, output: JSON.stringify(show(next), null, 2), metadata: {} }
          }
        }
      }),
  }),
)
