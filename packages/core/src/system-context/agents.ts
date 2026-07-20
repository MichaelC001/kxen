import os from "os"
import path from "path"
import { Effect, Layer, Schema } from "effect"
import { makeLocationNode } from "../effect/app-node"
import { Location } from "../location"
import { SystemContext } from "./index"
import { SystemContextRegistry } from "./registry"
import { scanAgentsDir, renderInjection } from "@kxen/agents"

// .agents/（项目级 + 用户级）作为 Context Source 注入 system-context。
// 内容为空时不注册，避免向模型注入空段。
const layer = Layer.effectDiscard(
  Effect.gen(function* () {
    const location = yield* Location.Service
    const registry = yield* SystemContextRegistry.Service

    const load = Effect.promise(async () => {
      const [projectDocs, userDocs] = await Promise.all([
        scanAgentsDir(path.join(location.project.directory, ".agents")),
        scanAgentsDir(path.join(os.homedir(), ".agents")),
      ])
      return renderInjection([...projectDocs, ...userDocs])
    })

    const context = SystemContext.make({
      key: SystemContext.Key.make("kxen/agents"),
      codec: Schema.toCodecJson(Schema.String),
      load: load.pipe(Effect.map((text) => (text.trim() === "" ? SystemContext.unavailable : text))),
      baseline: (text) => text,
      update: (_previous, text) => text,
    })

    yield* registry.register({ key: SystemContext.Key.make("kxen/agents"), load: Effect.succeed(context) })
  }),
)

export const node = makeLocationNode({
  name: "kxen-agents-context",
  layer,
  deps: [Location.node, SystemContextRegistry.node],
})
