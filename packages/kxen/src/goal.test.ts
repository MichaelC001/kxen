import { describe, expect, test, beforeAll, afterAll } from "bun:test"
import * as Goal from "./goal"
import { mkdtempSync, rmSync } from "fs"
import { tmpdir } from "os"
import path from "path"

const contract: Goal.GoalContract = {
  objective: "迁移完成",
  completionCriteria: "bun test 全绿",
  budget: { turns: 5, tokens: 1000 },
}

describe("状态机", () => {
  test("create -> active -> paused -> active -> complete", () => {
    let g = Goal.create(contract, "g1")
    expect(g.status).toBe("draft")
    g = Goal.activate(g)
    expect(g.status).toBe("active")
    g = Goal.pause(g)
    g = Goal.resume(g)
    g = Goal.complete(g, "测试输出 1074 pass")
    expect(g.status).toBe("complete")
    expect(g.verificationEvidence).toContain("1074")
  })

  test("契约不完整拒绝创建", () => {
    expect(() => Goal.create({ objective: "", completionCriteria: "x" }, "g2")).toThrow(Goal.GoalError)
    expect(() => Goal.create({ objective: "x", completionCriteria: "" }, "g2")).toThrow(Goal.GoalError)
  })

  test("非法转移报错", () => {
    const g = Goal.create(contract, "g3")
    expect(() => Goal.complete(g, "e")).toThrow(Goal.GoalError)
    expect(() => Goal.pause(g)).toThrow(Goal.GoalError)
  })

  test("complete 需要验证证据", () => {
    const g = Goal.activate(Goal.create(contract, "g4"))
    expect(() => Goal.complete(g, "")).toThrow(Goal.GoalError)
  })
})

describe("阻塞三次规则", () => {
  test("同一原因累计 3 次才 blocked", () => {
    let g = Goal.activate(Goal.create(contract, "g5"))
    g = Goal.recordTurn(g, { blockedReason: "网络不可达" })
    expect(g.status).toBe("active")
    g = Goal.recordTurn(g, { blockedReason: "网络不可达" })
    expect(g.status).toBe("active")
    g = Goal.recordTurn(g, { blockedReason: "网络不可达" })
    expect(g.status).toBe("blocked")
    expect(g.consecutiveBlocks).toBe(3)
  })

  test("不同原因重置计数", () => {
    let g = Goal.activate(Goal.create(contract, "g6"))
    g = Goal.recordTurn(g, { blockedReason: "A" })
    g = Goal.recordTurn(g, { blockedReason: "B" })
    expect(g.status).toBe("active")
    expect(g.consecutiveBlocks).toBe(1)
  })

  test("terminal 当轮 blocked", () => {
    let g = Goal.activate(Goal.create(contract, "g7"))
    g = Goal.recordTurn(g, { blockedReason: "目标矛盾", terminal: true })
    expect(g.status).toBe("blocked")
  })
})

describe("预算", () => {
  test("turns 耗尽进 budget_limited", () => {
    let g = Goal.activate(Goal.create(contract, "g8"))
    for (let i = 0; i < 5; i++) g = Goal.recordTurn(g, {})
    expect(g.status).toBe("budget_limited")
  })

  test("tokens 耗尽进 budget_limited", () => {
    let g = Goal.activate(Goal.create(contract, "g9"))
    g = Goal.recordTurn(g, { tokens: 600 })
    g = Goal.recordTurn(g, { tokens: 500 })
    expect(g.status).toBe("budget_limited")
  })
})

describe("持久化", () => {
  let dir: string
  beforeAll(() => {
    dir = mkdtempSync(path.join(tmpdir(), "kxen-goal-"))
  })
  afterAll(() => {
    rmSync(dir, { recursive: true, force: true })
  })

  test("save/load/list/remove", async () => {
    const g = Goal.activate(Goal.create(contract, "gx"))
    await Goal.save(dir, g)
    const loaded = await Goal.load(dir, "gx")
    expect(loaded.status).toBe("active")
    expect((await Goal.list(dir)).map((x) => x.id)).toContain("gx")
    await Goal.remove(dir, "gx")
    expect((await Goal.list(dir)).map((x) => x.id)).not.toContain("gx")
  })
})
