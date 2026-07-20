#!/usr/bin/env bun
// 上游 patch 的 scope 批量替换（docs/plan/01 第 5 节）。
// 用法: bun run script/sync-scope.ts < input.patch > output.patch
// 替换规则集中在此维护；保护清单（protected-features）的冲突在 apply 阶段人工处理。

const RULES: Array<[RegExp, string]> = [
  // workspace 包名
  [/@opencode-ai\//g, "@kxen/"],
  // 主包名（package.json name、bin、workspace 引用）
  [/"name": "opencode"/g, '"name": "kxen"'],
  [/"opencode": "workspace:\*"/g, '"kxen": "workspace:*"'],
  // scriptName / CLI 品牌
  [/\.scriptName\("opencode"\)/g, '.scriptName("kxen")'],
]

const input = await new Response(Bun.stdin.stream()).text()
let out = input
const stats: string[] = []
for (const [re, to] of RULES) {
  const count = (out.match(re) ?? []).length
  if (count > 0) stats.push(`${to} x${count}`)
  out = out.replace(re, to)
}
process.stderr.write(`sync-scope: ${stats.join(", ") || "no replacements"}\n`)
process.stdout.write(out)
