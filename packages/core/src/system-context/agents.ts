import os from "os"
import path from "path"
import { Effect, Layer, Schema } from "effect"
import { makeLocationNode } from "../effect/app-node"
import { Location } from "../location"
import { SystemContext } from "./index"
import { SystemContextRegistry } from "./registry"
import { scanAgentsDir, renderInjection } from "@kxen/agents"

// .agents/（项目级 + 用户级）作为 Context Source 注入 system-context。
// 启动时无文档则不注册：unavailable 阻塞 epoch 初始化、空 baseline 非法，
// 两者都违反 SystemContext 不变量。运行中新增 .agents/ 需重启 daemon 生效。
const layer = Layer.effectDiscard(
  Effect.gen(function* () {
    const location = yield* Location.Service
    const registry = yield* SystemContextRegistry.Service

    const scan = () =>
      Effect.promise(async () => {
        const [projectDocs, userDocs] = await Promise.all([
          scanAgentsDir(path.join(location.project.directory, ".agents")),
          scanAgentsDir(path.join(os.homedir(), ".agents")),
        ])
        return renderInjection([...projectDocs, ...userDocs])
      })

    const initial = yield* scan()
    if (initial.trim() === "") {
      yield* Effect.logDebug("kxen/agents: no .agents content, skipping registration")
      return
    }

    const context = SystemContext.make({
      key: SystemContext.Key.make("kxen/agents"),
      codec: Schema.toCodecJson(Schema.String),
      load: scan().pipe(Effect.map((text) => (text.trim() === "" ? SystemContext.unavailable : text))),
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
