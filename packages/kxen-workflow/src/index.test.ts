import { describe, expect, test } from "bun:test"
import { WorkflowRuntime, type SubagentResult } from "./index"

const fakeExecutor = async (prompt: string): Promise<SubagentResult> => ({ text: `reply:${prompt.slice(0, 20)}` })

describe("WorkflowRuntime", () => {
  test("agent 原语执行并计数", async () => {
    const rt = new WorkflowRuntime({ executeAgent: fakeExecutor })
    const state = await rt.run(`
      const r = await agent("hello")
      phase("p1")
      const r2 = await agent("world")
      return [r.text, r2.text]
    `)
    expect(state.status).toBe("completed")
    expect(state.agentCalls).toBe(2)
    expect(state.phases).toHaveLength(1)
    expect(state.result).toEqual(["reply:hello", "reply:world"])
  })

  test("pipeline 并发执行保序", async () => {
    const rt = new WorkflowRuntime({ executeAgent: fakeExecutor })
    const state = await rt.run(`
      const rs = await pipeline([1,2,3,4,5], async (n) => n * 2, { concurrency: 2 })
      return rs
    `)
    expect(state.result).toEqual([2, 4, 6, 8, 10])
  })

  test("超过 agent 上限报错", async () => {
    const rt = new WorkflowRuntime({ executeAgent: fakeExecutor, maxAgentCalls: 2 })
    const state = await rt.run(`
      await agent("a"); await agent("b"); await agent("c")
    `)
    expect(state.status).toBe("failed")
    expect(state.error).toContain("上限")
  })

  test("resume 回放缓存不重复执行", async () => {
    let calls = 0
    const rt = new WorkflowRuntime({
      executeAgent: async (p) => {
        calls++
        return { text: `r:${p}` }
      },
    })
    const first = await rt.run(`await agent("a"); throw new Error("boom")`)
    expect(first.status).toBe("failed")
    expect(calls).toBe(1)

    const snap = rt.snapshot(first.id)!
    const second = await rt.run(`const r = await agent("a"); return r.text`, undefined, {
      id: first.id,
      cache: snap.cache,
    })
    expect(second.status).toBe("completed")
    expect(second.result).toBe("r:a")
    expect(calls).toBe(1)
  })

  test("constraints 与 args 透传", async () => {
    const rt = new WorkflowRuntime({
      executeAgent: fakeExecutor,
      constraintsProvider: () => ({ maxConcurrent: 8 }),
    })
    const state = await rt.run(`return { c: constraints(), a: args }`, { tag: "x" })
    expect(state.result).toEqual({ c: { maxConcurrent: 8 }, a: { tag: "x" } })
  })

  test("pause 接口可调用且 run 状态完整", async () => {
    const rt = new WorkflowRuntime({ executeAgent: fakeExecutor })
    const state = await rt.run(`await agent("a")`)
    expect(state.status).toBe("completed")
    rt.pause(state.id)
    expect(rt.snapshot(state.id)?.state.id).toBe(state.id)
  })
})
