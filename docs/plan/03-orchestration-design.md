# M6 编排层设计定盘

日期: 2026-07-20
状态: 执行依据

## 关键架构事实（决定形态）

- opencode agent 配置已支持每个 agent 绑定独立 model（src/agent/agent.ts，mode: subagent/primary/all）——角色化路由的载体已存在，kxen 不重建。
- 模型调用链: session/agent -> @kxen/llm route/executor -> provider。MRM 挂在调用外层（acquire/release）与 agent spawn 的模型解析层。
- permission 层（src/permission/evaluate.ts）是规则评估管线——safety 以规则集挂入。
- core 的 system-context 有 Context Source Registry（docs/upstream/CONTEXT.md）——.agents/OKF 作为 Context Source 注入。
- task 工具（src/tool/task.ts）已是主 agent spawn subagent 的机制——角色化 subagent = 预设 agent 配置生成。

## 包边界

proof 要求七个独立包。落地:

| 包 | 位置 | 说明 |
| --- | --- | --- |
| kxen-auth | 主包 src/auth/import.ts + src/plugin/anthropic/claude.ts | M5 已完成 |
| packages/kxen-mrm | 独立包，import 主包服务 | 全局资源管理 |
| packages/kxen-goal | 独立包 | goal 状态机 + 预算 |
| packages/kxen-workflow | 独立包 | 脚本编排 runtime |
| packages/kxen-subagent | 独立包 | 角色 agent 配置生成 |
| packages/kxen-safety | 独立包 | 灾难规则集（permission 评估输入） |
| packages/kxen-agents | 独立包 | .agents/OKF 解析 -> Context Source |

## 角色配置（~/.config/kxen/config.toml 与项目 .kxen/config.toml）

```toml
[roles]
thinking = { provider = "anthropic", model = "claude-sonnet-4-5-20250929" }
planning = { provider = "anthropic", model = "claude-opus-4-8" }
execution = { provider = "xai", model = "grok-build-0.1" }
review = { provider = "kimi-for-coding", model = "k3" }
research = { provider = "openai", model = "gpt-5.4" }

[limits]
global_concurrent = 8

[limits.providers.anthropic]
concurrent = 4

[limits.providers.xai]
concurrent = 6

[[roles.fallback]]
role = "thinking"
chain = ["planning", "research"]
```

## MRM 行为

- acquire(role) -> 解析角色到 provider/model -> 检查 provider 并发/限额 -> 不足则按 fallback chain 降级 -> 排队或拒绝。
- release() 归还。状态查询 getStatus() 供注入规划模型上下文。
- 单 provider 单变体先行（变体后补，见规划 v1.1 决策）。

## 实施顺序

safety（最小） -> mrm（核心） -> subagent（角色配置生成） -> agents（OKF） -> goal（状态机） -> workflow（runtime）。

## 验证（proof 第 7 条「可演示的真实行为」）

- safety: 单元测试 + 拦截演示（rm -rf / 被拦）
- mrm: 并发上限生效演示 + 降级链演示 + 状态输出
- subagent: 角色 agent 配置生成并可被 task 工具使用
- agents: .agents 目录内容出现在 system context
- goal: 状态机流转（create -> active -> complete/blocked）+ 预算拦截
- workflow: 模型可调用 workflow 工具执行脚本（agent()/pipeline() 原语生效）
