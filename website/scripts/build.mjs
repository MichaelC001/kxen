import { readdirSync, statSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { extname, join, relative } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const limit = 500_000;
const result = spawnSync("pnpm", ["exec", "astro", "build"], {
  cwd: root,
  env: process.env,
  stdio: "inherit",
});

if (result.status !== 0) {
  process.exit(result.status ?? 1);
}

function files(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? files(path) : [path];
  });
}

const chunks = files(join(root, "dist", "_astro"))
  .filter((path) => extname(path) === ".js")
  .map((path) => ({ path, size: statSync(path).size }))
  .sort((left, right) => right.size - left.size);
const oversized = chunks.filter(({ size }) => size > limit);

for (const { path, size } of chunks) {
  process.stdout.write(`${relative(root, path)} ${size} bytes\n`);
}

if (oversized.length > 0) {
  throw new Error(
    `browser runtime chunks exceed ${limit} bytes: ${oversized
      .map(({ path, size }) => `${relative(root, path)}=${size}`)
      .join(", ")}`,
  );
}
