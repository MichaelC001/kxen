import path from "path"
import { Effect } from "effect"
import { HttpApiBuilder, HttpApiError } from "effect/unstable/httpapi"
import { Global } from "@kxen/core/global"
import * as Goal from "@kxen/goal"
import { InstanceHttpApi } from "../api"

const dir = () => path.join(Global.Path.data, "goals")

const toApi = (g: Goal.Goal) => ({
  id: g.id,
  contract: g.contract,
  status: g.status as string,
  createdAt: g.createdAt,
  updatedAt: g.updatedAt,
  turnsUsed: g.turnsUsed,
  tokensUsed: g.tokensUsed,
  consecutiveBlocks: g.consecutiveBlocks,
  blockReason: g.blockReason,
  verificationEvidence: g.verificationEvidence,
})

async function loadOrUndef(id: string) {
  return Goal.load(dir(), id).catch(() => undefined)
}

async function transit(id: string, fn: (g: Goal.Goal) => Goal.Goal) {
  const g = await loadOrUndef(id)
  if (!g) return undefined
  const next = fn(g)
  await Goal.save(dir(), next)
  return toApi(next)
}

export const goalHandlers = HttpApiBuilder.group(InstanceHttpApi, "goal", (handlers) =>
  Effect.gen(function* () {
    const runTransition = (id: string, fn: (g: Goal.Goal) => Goal.Goal) =>
      Effect.tryPromise({
        try: () => transit(id, fn),
        catch: () => new HttpApiError.BadRequest({}),
      }).pipe(
        Effect.flatMap((result) => (result ? Effect.succeed(result) : Effect.fail(new HttpApiError.NotFound({})))),
      )

    const list = Effect.fn("GoalHttpApi.list")(function* () {
      const goals = yield* Effect.promise(() => Goal.list(dir()))
      return goals.map(toApi)
    })

    const create = Effect.fn("GoalHttpApi.create")(function* (ctx: { payload: Goal.GoalContract }) {
      const g = yield* Effect.try({
        try: () => Goal.create(ctx.payload, `goal_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`),
        catch: () => new HttpApiError.BadRequest({}),
      })
      yield* Effect.promise(() => Goal.save(dir(), g))
      return toApi(g)
    })

    const get = Effect.fn("GoalHttpApi.get")(function* (ctx: { params: { id: string } }) {
      const g = yield* Effect.promise(() => loadOrUndef(ctx.params.id))
      if (!g) return yield* Effect.fail(new HttpApiError.NotFound({}))
      return toApi(g)
    })

    const complete = Effect.fn("GoalHttpApi.complete")(function* (ctx: {
      params: { id: string }
      payload: { evidence: string }
    }) {
      return yield* runTransition(ctx.params.id, (g) => Goal.complete(g, ctx.payload.evidence))
    })

    return handlers
      .handle("list", list)
      .handle("create", create)
      .handle("get", get)
      .handle("activate", (ctx: { params: { id: string } }) => runTransition(ctx.params.id, Goal.activate))
      .handle("pause", (ctx: { params: { id: string } }) => runTransition(ctx.params.id, Goal.pause))
      .handle("resume", (ctx: { params: { id: string } }) => runTransition(ctx.params.id, Goal.resume))
      .handle("complete", complete)
      .handle("cancel", (ctx: { params: { id: string } }) => runTransition(ctx.params.id, Goal.cancel))
  }),
)
