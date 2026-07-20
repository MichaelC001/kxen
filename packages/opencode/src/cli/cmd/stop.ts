import { Effect } from "effect"
import { effectCmd } from "../effect-cmd"
import { alive, readState, pidFile } from "./start"

export const StopCommand = effectCmd({
  command: "stop",
  describe: "stop kxen daemon and web interface",
  instance: false,
  handler: Effect.fn("Cli.stop")(function* () {
    const state = yield* Effect.promise(() => readState())
    if (!state) {
      console.log("kxen is not running (no state file)")
      return
    }
    for (const [name, pid] of [
      ["web", state.webPid],
      ["serve", state.servePid],
    ] as const) {
      if (pid > 0 && (yield* Effect.promise(() => alive(pid)))) {
        yield* Effect.sync(() => {
          try {
            process.kill(pid, "SIGTERM")
          } catch {}
        })
        console.log(`stopped ${name} (pid ${pid})`)
      }
    }
    yield* Effect.promise(() => Bun.file(pidFile()).delete().catch(() => {}))
  }),
})
