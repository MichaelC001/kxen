// 角色化 subagent：把 MRM 的角色绑定落成 opencode agent 定义。
// opencode 原生支持每 agent 独立 model（src/agent/agent.ts），本包只做
// 角色 -> agent 配置的生成与权限预设，调度经 task 工具自然发生。

import type { MrmConfig } from "@kxen/mrm"

export interface RoleAgentDef {
  name: string
  description: string
  mode: "subagent"
  model: { providerID: string; modelID: string }
  /** 简化权限：readonly 只读工具；full 全工具 */
  permissionProfile: "readonly" | "readonly-todo" | "full"
  prompt: string
}

const READONLY_TOOLS = "read / grep / glob / list / webfetch / websearch"
const PROFILES: Record<string, { profile: RoleAgentDef["permissionProfile"]; duty: string }> = {
  thinking: {
    profile: "readonly",
    duty: `深度分析与方案评估。你只能使用只读工具（${READONLY_TOOLS}），输出结论与理由，不修改任何文件。`,
  },
  planning: {
    profile: "readonly-todo",
    duty: `任务拆解与执行计划。你主要使用只读工具（${READONLY_TOOLS}）理解现状，输出分步计划（可用 todo 工具记录），不直接改代码。`,
  },
  execution: {
    profile: "full",
    duty: "高速执行既定计划：按任务直接修改文件、运行命令、验证结果。不做额外设计决策，遇到计划外分歧时停下来报告。",
  },
  review: {
    profile: "readonly",
    duty: `对抗性审查：找出改动中的 bug、回归与遗漏。只读工具（${READONLY_TOOLS}），输出按严重度排序的问题清单。`,
  },
  research: {
    profile: "readonly",
    duty: `资料调研：搜索、阅读、交叉验证外部信息与代码事实。只读工具（${READONLY_TOOLS}），输出带来源的结论。`,
  },
}

const FALLBACK_DUTY: RoleAgentDef["prompt"] = "完成主代理委派的子任务，遵循其指令边界。"

export function roleAgents(config: Pick<MrmConfig, "roles">): RoleAgentDef[] {
  return Object.entries(config.roles).map(([role, binding]) => {
    const preset = PROFILES[role]
    return {
      name: `kxen-${role}`,
      description: `kxen ${role} agent (${binding.provider}/${binding.model})${preset ? "" : " - custom role"}`,
      mode: "subagent" as const,
      model: { providerID: binding.provider, modelID: binding.model },
      permissionProfile: preset?.profile ?? "full",
      prompt: preset?.duty ?? FALLBACK_DUTY,
    }
  })
}

/** 权限预设 -> opencode permission 配置片段 */
export function permissionConfig(profile: RoleAgentDef["permissionProfile"]): Record<string, string> {
  switch (profile) {
    case "readonly":
      return {
        "*": "deny",
        read: "allow",
        grep: "allow",
        glob: "allow",
        list: "allow",
        webfetch: "allow",
        websearch: "allow",
      }
    case "readonly-todo":
      return { ...permissionConfig("readonly"), todowrite: "allow" }
    case "full":
      return { "*": "allow" }
  }
}
