import { Effect, Schema, Stream } from "effect"
import { ChildProcess } from "effect/unstable/process"
import { ChildProcessSpawner } from "effect/unstable/process/ChildProcessSpawner"
import * as Tool from "./tool"
import { Shell } from "@kxen/core/shell"
import { evaluateShellCommand } from "@kxen/safety"
import { InstanceState } from "@/effect/instance-state"

const SHELL_TYPES = ["zsh", "bash", "fish", "cmd", "powershell"] as const
type ShellType = (typeof SHELL_TYPES)[number]

export const Parameters = Schema.Struct({
  type: Schema.Literals(SHELL_TYPES).annotate({
    description:
      "REQUIRED shell dialect. Think about the target environment FIRST, then write the command for that dialect. Each has different syntax for variables, arrays, chaining, and redirection.",
  }),
  path: Schema.String.annotate({ description: "Working directory for the command." }),
  command: Schema.String.annotate({ description: "Command text written in the dialect of `type`." }),
  timeout: Schema.optional(Schema.Finite).annotate({ description: "Timeout in milliseconds (default 120000)." }),
})
export type Parameters = Schema.Schema.Type<typeof Parameters>

// X3 方言校验器：命中即拒绝 + 纠正文案。
const DIALECT_RULES: Array<{ type: ShellType; re: RegExp; hint: string }> = [
  { type: "fish", re: /^\s*export\s+[A-Za-z_][A-Za-z0-9_]*=/m, hint: "fish has no `export`. Use `set -x NAME value`." },
  { type: "fish", re: /\$\{[^}]+\[@\]\}/, hint: "fish arrays use `$name` (all elements), not `${name[@]}`." },
  { type: "cmd", re: /^\s*export\s+/m, hint: "cmd has no `export`. Use `set NAME=value`." },
  { type: "cmd", re: /\bsudo\b/, hint: "cmd has no sudo. Run as administrator instead." },
  { type: "powershell", re: /^\s*export\s+/m, hint: "PowerShell has no `export`. Use `$env:NAME = 'value'`." },
  { type: "zsh", re: /\$\{?[A-Za-z_][A-Za-z0-9_]*\[0\]/, hint: "zsh arrays are 1-indexed, not 0-indexed." },
]

export function validateDialect(type: ShellType, command: string): string | undefined {
  for (const rule of DIALECT_RULES) {
    if (rule.type === type && rule.re.test(command)) return rule.hint
  }
  return undefined
}

const TYPE_BIN: Record<ShellType, string> = {
  zsh: "zsh",
  bash: "bash",
  fish: "fish",
  cmd: process.platform === "win32" ? (process.env.COMSPEC ?? "cmd.exe") : "cmd",
  powershell: process.platform === "win32" ? "powershell.exe" : "pwsh",
}

// X2 方言卡片（静态摘要；完整差异由 type 必填强制模型先思考）
const DESCRIPTION = [
  "Execute a command in an explicitly declared shell dialect.",
  "Dialect notes:",
  "- zsh/bash: POSIX syntax; arrays differ (zsh is 1-indexed); `&&`/`||` work; rc files are sourced.",
  "- fish: no `export` (use `set -x`); different array and substitution syntax; `&&` works in fish 3+.",
  "- cmd: Windows only; `set NAME=value`; no sudo; `&` chains (not `&&` on old versions).",
  "- powershell: `$env:NAME='value'`; `&&` only in pwsh 7+; prefer `;` to chain.",
  "Prefer one well-formed command per call over long chained one-liners: a single failing segment is easier to read than five joined commands.",
].join("\n")

export const ExecTool = Tool.define(
  "exec",
  Effect.gen(function* () {
    const spawner = yield* ChildProcessSpawner

    return {
      description: DESCRIPTION,
      parameters: Parameters,
      execute: (params: Parameters, ctx: Tool.Context) =>
        Effect.gen(function* () {
          const instanceCtx = yield* InstanceState.context

          const hint = validateDialect(params.type, params.command)
          if (hint) throw new Error(`Dialect mismatch for ${params.type}: ${hint}`)

          const cwd = params.path.startsWith("/") || /^[A-Za-z]:/.test(params.path)
            ? params.path
            : `${instanceCtx.directory}/${params.path}`

          // kxen-safety：与 shell 工具同一道硬拦截
          const verdict = evaluateShellCommand(params.command, cwd)
          if (verdict.verdict === "deny") {
            throw new Error(
              `Blocked by safety rule ${verdict.ruleId}: ${verdict.reason}${verdict.suggestion ? ` Suggestion: ${verdict.suggestion}` : ""}`,
            )
          }

          const bin = TYPE_BIN[params.type]
          const args = Shell.args(bin, params.command, cwd)
          const timeout = params.timeout ?? 120_000

          const [raw, exitCode] = yield* Effect.scoped(
            Effect.gen(function* () {
              const handle = yield* spawner.spawn(ChildProcess.make(bin, args, { cwd, extendEnv: true, stdin: "ignore" }))
              const collectText = Effect.gen(function* () {
                let text = ""
                yield* Stream.runForEach(Stream.decodeText(handle.all), (chunk) =>
                  Effect.sync(() => {
                    text += chunk
                  }),
                )
                return text
              })
              const timed = Effect.all([collectText, handle.exitCode], { concurrency: "unbounded" }).pipe(
                Effect.map(([text, code]) => [text, code] as const),
              )
              const onTimeout = Effect.sleep(timeout).pipe(Effect.as(["(timed out)", 124] as const))
              return yield* Effect.raceFirst(timed, onTimeout)
            }),
          ).pipe(Effect.orDie)

          const truncated = raw.length > 30_000 ? raw.slice(0, 30_000) + "\n... (truncated)" : raw
          return {
            title: `${params.type}: ${params.command.slice(0, 60)}`,
            output: exitCode === 0 ? truncated : `exit ${exitCode}\n${truncated}`,
            metadata: { directory: cwd, exitCode, shell: params.type },
          }
        }),
    }
  }),
)
