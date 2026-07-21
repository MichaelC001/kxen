import { describe, expect, test } from "bun:test"
import { ModelResourceManager, parseConfig } from "./index"

const TOML = `
[roles]
thinking = { provider = "anthropic", model = "claude-sonnet-4-5" }
execution = { provider = "xai", model = "grok-build-0.1" }
review = { provider = "kimi-for-coding", model = "k3" }

[[roles.fallback]]
role = "thinking"
chain = ["execution"]

[limits]
global_concurrent = 2

[limits.providers.anthropic]
concurrent = 1
`

describe("parseConfig", () => {
  test("解析 roles/limits/fallback", () => {
    const c = parseConfig(TOML)
    expect(c.roles.thinking).toEqual({ provider: "anthropic", model: "claude-sonnet-4-5" })
    expect(c.fallbacks.thinking).toEqual(["execution"])
    expect(c.globalConcurrent).toBe(2)
    expect(c.providers.anthropic?.concurrent).toBe(1)
  })
})

describe("resolve 与降级", () => {
  test("首选可用直取", () => {
    const mrm = new ModelResourceManager(parseConfig(TOML))
    expect(mrm.resolve("thinking")).toEqual({
      provider: "anthropic",
      model: "claude-sonnet-4-5",
      degradedFrom: undefined,
    })
  })
  test("首选满员走降级链", () => {
    const mrm = new ModelResourceManager(parseConfig(TOML))
    const release = mrm.tryAcquire("anthropic")
    expect(release).toBeDefined()
    expect(mrm.resolve("thinking")).toEqual({ provider: "xai", model: "grok-build-0.1", degradedFrom: "thinking" })
    release!()
  })
  test("全链满员返回 undefined", () => {
    const mrm = new ModelResourceManager(parseConfig(TOML))
    mrm.tryAcquire("anthropic")
    mrm.tryAcquire("xai")
    // global = 2 已满
    expect(mrm.resolve("thinking")).toBeUndefined()
  })
})

describe("并发槽", () => {
  test("超限 tryAcquire 返回 undefined，release 后恢复", () => {
    const mrm = new ModelResourceManager(parseConfig(TOML))
    const r1 = mrm.tryAcquire("anthropic")
    expect(mrm.tryAcquire("anthropic")).toBeUndefined()
    r1!()
    expect(mrm.tryAcquire("anthropic")).toBeDefined()
  })
  test("acquire 排队，释放后唤醒", async () => {
    const mrm = new ModelResourceManager(parseConfig(TOML))
    const r1 = mrm.tryAcquire("anthropic")!
    let woke = false
    const p = mrm.acquire("anthropic").then((release) => {
      woke = true
      return release
    })
    expect(woke).toBe(false)
    r1()
    const r2 = await p
    expect(woke).toBe(true)
    r2()
  })
})

describe("status/describe", () => {
  test("状态快照", () => {
    const mrm = new ModelResourceManager(parseConfig(TOML))
    mrm.tryAcquire("xai")
    const s = mrm.status()
    expect(s.providers.xai?.inFlight).toBe(1)
    expect(s.global.inFlight).toBe(1)
    expect(mrm.describe()).toContain("xai: 1/")
  })
})
