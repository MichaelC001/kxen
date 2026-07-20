import { describe, expect, test, beforeAll, afterAll } from "bun:test"
import { scanAgentsDir, renderInjection, globMatch } from "./index"
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "fs"
import { tmpdir } from "os"
import path from "path"

let dir: string

beforeAll(() => {
  dir = mkdtempSync(path.join(tmpdir(), "kxen-agents-"))
  mkdirSync(path.join(dir, "rules"), { recursive: true })
  mkdirSync(path.join(dir, "references"), { recursive: true })
  writeFileSync(
    path.join(dir, "rules", "build.md"),
    `---
type: rule
title: 构建命令
description: bun 构建与测试命令
priority: high
alwaysApply: true
---
构建用 bun run build，测试用 bun test。`,
  )
  writeFileSync(
    path.join(dir, "rules", "scoped.md"),
    `---
type: rule
title: 包内规则
applyTo: ["packages/**"]
---
只在 packages 内生效。`,
  )
  writeFileSync(
    path.join(dir, "references", "api.md"),
    `---
type: reference
title: API 约定
description: REST 端点命名与错误码
---
端点一律复数名词。`,
  )
  writeFileSync(path.join(dir, "index.md"), "---\nokf_version: '0.1'\n---\n# index\n")
})

afterAll(() => {
  rmSync(dir, { recursive: true, force: true })
})

describe("scanAgentsDir", () => {
  test("解析 frontmatter 与类型", async () => {
    const docs = await scanAgentsDir(dir)
    expect(docs).toHaveLength(3)
    const build = docs.find((d) => d.path.includes("build.md"))!
    expect(build.type).toBe("rule")
    expect(build.priority).toBe("high")
    expect(build.alwaysApply).toBe(true)
    expect(build.body).toContain("bun run build")
  })

  test("index.md 与 log.md 不进入结果", async () => {
    const docs = await scanAgentsDir(dir)
    expect(docs.every((d) => d.path !== "index.md")).toBe(true)
  })

  test("不存在目录返回空", async () => {
    expect(await scanAgentsDir(path.join(dir, "nope"))).toEqual([])
  })
})

describe("globMatch", () => {
  test("** 与 * 匹配", () => {
    expect(globMatch("packages/**", "packages/core/src")).toBe(true)
    expect(globMatch("packages/**", "src")).toBe(false)
    expect(globMatch("*.ts", "a.ts")).toBe(true)
  })
})

describe("renderInjection", () => {
  test("alwaysApply rule 全文 + 其余进索引", async () => {
    const docs = await scanAgentsDir(dir)
    const out = renderInjection(docs)
    expect(out).toContain("## .agents 规则（自动注入）")
    expect(out).toContain("bun run build")
    expect(out).toContain("[reference] API 约定")
    expect(out).toContain("REST 端点命名与错误码")
    // scoped.md 无 alwaysApply 且无 cwd 命中 -> 只在索引
    expect(out).toContain("[rule] 包内规则")
    expect(out).not.toContain("只在 packages 内生效。")
  })

  test("applyTo 命中 cwd 时注入全文", async () => {
    const docs = await scanAgentsDir(dir)
    const out = renderInjection(docs, { cwd: "packages/core" })
    expect(out).toContain("只在 packages 内生效。")
  })

  test("roles 过滤", async () => {
    mkdirSync(path.join(dir, "rules2"), { recursive: true })
    writeFileSync(
      path.join(dir, "rules2", "r.md"),
      "---\ntype: rule\nalwaysApply: true\nroles: [review]\n---\n仅 review 可见。",
    )
    const docs = await scanAgentsDir(dir)
    expect(renderInjection(docs, { role: "execution" })).not.toContain("仅 review 可见。")
    expect(renderInjection(docs, { role: "review" })).toContain("仅 review 可见。")
  })
})
