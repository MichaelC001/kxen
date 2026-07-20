import { Effect } from "effect"
import { effectCmd } from "../effect-cmd"
import { Global } from "@kxen/core/global"
import path from "path"
import open from "open"

export type KxenProcesses = {
  servePid: number
  webPid: number
  servePort: number
  webPort: number
  startedAt: string
}

export const SERVE_PORT = 4096
export const WEB_PORT = 3000

export const pidFile = () => path.join(Global.Path.state, "kxen.json")

export const repoEntry = () =>
  // 开发态 argv[1] 是 packages/opencode/src/index.ts；生产态（编译二进制）为空
  process.argv[1]?.endsWith(".ts") ? process.argv[1] : undefined

export const repoRoot = () => {
  const entry = repoEntry()
  return entry ? path.resolve(path.dirname(entry), "../../..") : process.cwd()
}

export async function readState(): Promise<KxenProcesses | undefined> {
  const file = Bun.file(pidFile())
  if (!(await file.exists())) return undefined
  return file.json().catch(() => undefined)
}

export async function writeState(state: KxenProcesses) {
  await Bun.write(pidFile(), JSON.stringify(state, null, 2))
}

export async function alive(pid: number) {
  try {
    process.kill(pid, 0)
    return true
  } catch {
    return false
  }
}

export async function portReady(port: number) {
  try {
    const res = await fetch(`http://127.0.0.1:${port}/config`, { signal: AbortSignal.timeout(1500) })
    return res.ok
  } catch {
    return false
  }
}

export const StartCommand = effectCmd({
  command: ["$0", "start"],
  describe: "start kxen daemon and web interface",
  instance: false,
  builder: (yargs) =>
    yargs.option("serve-port", { type: "number", default: SERVE_PORT }).option("web-port", {
      type: "number",
      default: WEB_PORT,
    }),
  handler: Effect.fn("Cli.start")(function* (args) {
    const existing = yield* Effect.promise(() => readState())
    if (existing && (yield* Effect.promise(() => alive(existing.servePid)))) {
      console.log(`kxen is already running (serve :${existing.servePort}, web :${existing.webPort})`)
      return
    }

    const env = { ...process.env, OPENCODE_DISABLE_SHARE: "1" }
    const entry = repoEntry()
    const serveArgs = entry
      ? [process.execPath, entry, "serve", "--port", String(args.servePort)]
      : [process.execPath, "serve", "--port", String(args.servePort)]

    const serve = Bun.spawn(serveArgs, { stdio: ["ignore", "ignore", "inherit"], env })
    serve.unref()

    // vite dev（开发态）；生产态静态托管留待打包管线
    let webPid = 0
    if (entry) {
      const web = Bun.spawn([process.execPath, "--cwd", path.join(repoRoot(), "packages/app"), "dev"], {
        stdio: ["ignore", "ignore", "inherit"],
        env,
      })
      web.unref()
      webPid = web.pid
    }

    yield* Effect.promise(() =>
      writeState({
        servePid: serve.pid,
        webPid,
        servePort: args.servePort,
        webPort: args.webPort,
        startedAt: new Date().toISOString(),
      }),
    )

    // 等 daemon 就绪后开浏览器
    for (let i = 0; i < 30; i++) {
      if (yield* Effect.promise(() => portReady(args.servePort))) break
      yield* Effect.sleep("500 millis")
    }
    const url = `http://localhost:${args.webPort}`
    console.log(`kxen daemon: http://127.0.0.1:${args.servePort}`)
    console.log(`kxen web:    ${url}`)
    yield* Effect.promise(() => open(url).catch(() => {}))
  }),
})
