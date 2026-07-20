import { Schema } from "effect"
import { HttpApi, HttpApiEndpoint, HttpApiError, HttpApiGroup, OpenApi } from "effect/unstable/httpapi"
import { WorkspaceRoutingMiddleware } from "../middleware/workspace-routing"
import { described } from "./metadata"

const root = "/goal"

const Contract = Schema.Struct({
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
})

const Goal = Schema.Struct({
  id: Schema.String,
  contract: Contract,
  status: Schema.String,
  createdAt: Schema.Finite,
  updatedAt: Schema.Finite,
  turnsUsed: Schema.Finite,
  tokensUsed: Schema.Finite,
  consecutiveBlocks: Schema.Finite,
  blockReason: Schema.optional(Schema.String),
  verificationEvidence: Schema.optional(Schema.String),
})

const GoalNotFound = HttpApiError.NotFound.annotate({ identifier: "GoalNotFound" })

export const GoalApi = HttpApi.make("goal")
  .add(
    HttpApiGroup.make("goal")
      .add(
        HttpApiEndpoint.get("list", root, {
          success: described(Schema.Array(Goal), "List of goals"),
        }).annotateMerge(OpenApi.annotations({ identifier: "goal.list", summary: "List goals" })),
        HttpApiEndpoint.post("create", root, {
          payload: Contract,
          success: described(Goal, "Created goal"),
          error: [HttpApiError.BadRequest],
        }).annotateMerge(OpenApi.annotations({ identifier: "goal.create", summary: "Create goal" })),
        HttpApiEndpoint.get("get", `${root}/:id`, {
          params: { id: Schema.String },
          success: described(Goal, "Goal detail"),
          error: [GoalNotFound],
        }).annotateMerge(OpenApi.annotations({ identifier: "goal.get", summary: "Get goal" })),
        HttpApiEndpoint.post("activate", `${root}/:id/activate`, {
          params: { id: Schema.String },
          success: described(Goal, "Goal activated"),
          error: [HttpApiError.BadRequest, GoalNotFound],
        }).annotateMerge(OpenApi.annotations({ identifier: "goal.activate", summary: "Activate goal" })),
        HttpApiEndpoint.post("pause", `${root}/:id/pause`, {
          params: { id: Schema.String },
          success: described(Goal, "Goal paused"),
          error: [HttpApiError.BadRequest, GoalNotFound],
        }).annotateMerge(OpenApi.annotations({ identifier: "goal.pause", summary: "Pause goal" })),
        HttpApiEndpoint.post("resume", `${root}/:id/resume`, {
          params: { id: Schema.String },
          success: described(Goal, "Goal resumed"),
          error: [HttpApiError.BadRequest, GoalNotFound],
        }).annotateMerge(OpenApi.annotations({ identifier: "goal.resume", summary: "Resume goal" })),
        HttpApiEndpoint.post("complete", `${root}/:id/complete`, {
          params: { id: Schema.String },
          payload: Schema.Struct({ evidence: Schema.String }),
          success: described(Goal, "Goal completed"),
          error: [HttpApiError.BadRequest, GoalNotFound],
        }).annotateMerge(OpenApi.annotations({ identifier: "goal.complete", summary: "Complete goal" })),
        HttpApiEndpoint.post("cancel", `${root}/:id/cancel`, {
          params: { id: Schema.String },
          success: described(Goal, "Goal canceled"),
          error: [HttpApiError.BadRequest, GoalNotFound],
        }).annotateMerge(OpenApi.annotations({ identifier: "goal.cancel", summary: "Cancel goal" })),
      )
      .annotateMerge(OpenApi.annotations({ title: "goal", description: "kxen goal lifecycle routes." })),
  )
  .middleware(WorkspaceRoutingMiddleware)
