import { Effect } from "effect"
import { effectCmd } from "../effect-cmd"

// kxen 自更新（源码形态）：git pull --ff-only + bun install。
// npm/brew 等发布渠道尚未建立，建立后在此扩展。
export const UpgradeCommand = effectCmd({
  command: "upgrade [target]",
  describe: "upgrade kxen to the latest or a specific version",
  instance: false,
  builder: (yargs) =>
    yargs
      .positional("target", {
        describe: "branch, tag or commit to check out (default: fast-forward main)",
        type: "string",
      })
      .option("dir", {
        describe: "source directory (default: repository of the running CLI)",
        type: "string",
      }),
  handler: Effect.fn("Cli.upgrade")(function* (args) {
    const run = (cmd: string, argv: string[], cwd: string) =>
      Effect.promise(async () => {
        const proc = Bun.spawn([cmd, ...argv], { cwd, stdio: ["ignore", "inherit", "inherit"] })
        const exit = await proc.exited
        if (exit !== 0) throw new Error(`${cmd} ${argv.join(" ")} failed (exit ${exit})`)
      })

    // 源码目录：--dir 或 argv[1]（src/index.ts）推导的仓库根
    const dir = args.dir
      ? args.dir
      : process.argv[1]?.endsWith(".ts")
        ? `${process.argv[1]}/../../..`
        : process.cwd()

    const isRepo = yield* Effect.promise(() => Bun.file(`${dir}/.git`).exists())
    if (!isRepo) {
      console.error(
        `kxen source not found at ${dir}. Re-run the installer: curl -fsSL https://raw.githubusercontent.com/StringKe/kxen/main/install | bash`,
      )
      process.exitCode = 1
      return
    }

    console.log(`upgrading kxen in ${dir} ...`)
    if (args.target) {
      yield* run("git", ["fetch", "origin", "--depth", "50", args.target], dir)
      yield* run("git", ["checkout", "FETCH_HEAD"], dir)
    } else {
      yield* run("git", ["pull", "--ff-only"], dir)
    }
    yield* run("bun", ["install"], dir)
    console.log("done. restart kxen to use the new version.")
  }),
})
