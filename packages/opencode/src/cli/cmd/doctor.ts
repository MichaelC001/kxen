import { Effect } from "effect"
import { effectCmd } from "../effect-cmd"
import { Global } from "@kxen/core/global"
import path from "path"
import { portReady, readState, SERVE_PORT, WEB_PORT } from "./start"
import { importSubscriptions } from "../../auth/import"

type AuthEntry = {
  type: string
  access?: string
  key?: string
  expires?: number
  accountId?: string
}

export const DoctorCommand = effectCmd({
  command: "doctor",
  describe: "check kxen environment health",
  instance: false,
  handler: Effect.fn("Cli.doctor")(function* () {
    const rows: Array<[string, string]> = []
    const push = (k: string, v: string) => rows.push([k, v])

    push("bun", Bun.version)
    push("data dir", Global.Path.data)
    push("config dir", Global.Path.config)

    // 凭证状态（先执行订阅导入：官方 CLI 的新鲜副本优先）
    const imported = yield* Effect.promise(() => importSubscriptions().catch(() => []))
    for (const r of imported) {
      if (r.action === "imported") push(`import ${r.provider}`, "updated from official CLI")
    }
    const authFile = Bun.file(path.join(Global.Path.data, "auth.json"))
    if (yield* Effect.promise(() => authFile.exists())) {
      const auths = (yield* Effect.promise(() => authFile.json().catch(() => ({})))) as Record<string, AuthEntry>
      const now = Date.now()
      for (const [provider, entry] of Object.entries(auths)) {
        if (entry.type === "oauth") {
          const expired = typeof entry.expires === "number" && entry.expires > 0 && entry.expires < now
          push(`auth ${provider}`, expired ? "oauth (expired, will refresh)" : "oauth")
        } else {
          push(`auth ${provider}`, entry.type)
        }
      }
      if (Object.keys(auths).length === 0) push("auth", "auth.json is empty")
    } else {
      push("auth", "no auth.json (run providers login or import)")
    }

    // 运行状态
    const state = yield* Effect.promise(() => readState())
    if (state) push("state", `serve pid ${state.servePid}, web pid ${state.webPid}`)
    push(`port ${SERVE_PORT}`, (yield* Effect.promise(() => portReady(SERVE_PORT))) ? "daemon responding" : "no daemon")
    if (yield* Effect.promise(() => portReady(WEB_PORT))) push(`port ${WEB_PORT}`, "web responding")

    const width = Math.max(...rows.map(([k]) => k.length))
    for (const [k, v] of rows) console.log(`${k.padEnd(width)}  ${v}`)
  }),
})
