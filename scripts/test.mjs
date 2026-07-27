import { spawnSync } from "node:child_process";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const coverageEnabled = process.argv.includes("--coverage");

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: root,
    env: process.env,
    stdio: "inherit",
  });
  if (result.status !== 0) {
    throw new Error(`${command} exited with status ${result.status ?? 1}`);
  }
}

run("bash", ["scripts/check-lines.sh"]);

const args = ["exec", "vitest", "run", "--fileParallelism=false", "--maxWorkers=1"];
if (coverageEnabled) {
  args.push(
    "--coverage.enabled",
    "--coverage.provider=istanbul",
    "--coverage.reporter=text",
    "--coverage.reporter=json",
    "--coverage.reportsDirectory=coverage",
    "--coverage.thresholds.lines=80",
    "--coverage.thresholds.functions=80",
    "--coverage.thresholds.statements=80",
    "--coverage.thresholds.branches=70",
  );
}
run("pnpm", args);
