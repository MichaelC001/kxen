import { describe, expect, test } from "bun:test"
import { roleAgents, permissionConfig } from "./index"
import { parseConfig } from "@kxen/mrm"

const TOML = `
[roles]
thinking = { provider = "anthropic", model = "claude-sonnet-4-5" }
execution = { provider = "xai", model = "grok-build-0.1" }
`

describe("roleAgents", () => {
  const agents = roleAgents(parseConfig(TOML))

  test("每个角色生成一个 subagent 定义", () => {
    expect(agents).toHaveLength(2)
    const thinking = agents.find((a) => a.name === "kxen-thinking")!
    expect(thinking.mode).toBe("subagent")
    expect(thinking.model).toEqual({ providerID: "anthropic", modelID: "claude-sonnet-4-5" })
    expect(thinking.permissionProfile).toBe("readonly")
  })

  test("execution 是全工具权限", () => {
    const execution = agents.find((a) => a.name === "kxen-execution")!
    expect(execution.permissionProfile).toBe("full")
    expect(permissionConfig("full")).toEqual({ "*": "allow" })
  })

  test("readonly 权限只放行只读工具", () => {
    const p = permissionConfig("readonly")
    expect(p["*"]).toBe("deny")
    expect(p.read).toBe("allow")
    expect(p.bash).toBeUndefined()
  })
})
