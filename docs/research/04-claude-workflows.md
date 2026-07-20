# Claude Code Dynamic Workflows 机制理解

- 整理日期: 2026-07-20
- 来源: 用户提供的 Claude Code 官方文档全文（https://code.claude.com/docs/ 的 dynamic workflows 页，v2.1.154+）
- 定位: kxen Dynamic Workflow 功能的对标蓝本

## 1. 本质

Dynamic Workflow 是「编排逻辑代码化」：Claude 根据任务描述写一个 JavaScript 脚本，由独立 runtime 在后台执行；脚本自己持有循环、分支、并行与中间结果，主会话上下文只收最终结果。

与相近概念的边界（官方对比）：

| | Subagents | Skills | Agent teams | Workflows |
| --- | --- | --- | --- | --- |
| 谁在决定下一步 | Claude 逐轮决定 | Claude 按指令 | lead agent 逐轮 | 脚本 |
| 中间结果在哪 | 上下文窗口 | 上下文窗口 | 共享任务列表 | 脚本变量 |
| 可重复的是什么 | worker 定义 | 指令 | 团队定义 | 编排本身 |
| 规模 | 每轮几个 | 同左 | 几个长跑 peer | 每轮几十到几百 |
| 中断后 | 重开该轮 | 重开该轮 | 队友继续跑 | 同会话内可恢复 |

关键价值不只是「跑更多 agent」，而是可重复的质量模式：独立 agent 对抗互审、多角度起草再权衡，结果比单遍更可信。

## 2. 触发方式

- 提示词里直接要求（关键词 `ultracode`，或自然语言「use a workflow」）
- `/effort ultracode`：每个实质任务都自动规划 workflow（xhigh 推理档）
- 运行已存在的 workflow 命令：内置 `/deep-research`，或用户自己保存的

## 3. 审批与权限

- CLI 按 permission mode 提示：default / acceptEdits 每次问（可选「本项目不再问」）；auto 只在首次问；bypass / `-p` / SDK 不问
- 提示可查看原始脚本（Ctrl+G 进编辑器，Tab 调整提示词）
- workflow 派生的 subagent 始终以 acceptEdits 运行并继承工具 allowlist；文件编辑自动通过，未加白名单的 shell / web / MCP 调用仍会中途询问

## 4. 脚本模型

- 顶层 `await` 的 JavaScript；`agent(prompt, opts)` 派一个 subagent，`pipeline(items, fn)` 对列表每项派一个
- 可用 `schema` 参数要结构化返回
- `args` 全局变量接收调用方传入的结构化输入
- 保存：`/workflows` 里选中 run 按 `s`，存到项目 `.claude/workflows/`（随仓库共享）或个人 `~/.claude/workflows/`，之后作为 `/<name>` 命令复用
- 脚本落盘在 `~/.claude/projects/` 的会话目录下，可 diff、可改后重跑

示例形态（官方示例）：

```js
export const meta = { name: 'audit-routes', description: '...' }
const found = await agent('List every .ts file under src/routes/.', { schema: {...} })
const audits = await pipeline(found.files, file =>
  agent(`Audit ${file} for missing authentication checks.`, { label: file }))
return audits.filter(Boolean)
```

## 5. 运行时约束（官方硬限制）

| 约束 | 值 / 说明 |
| --- | --- |
| 中途用户输入 | 不支持；只有 agent 权限提示能暂停 run |
| 脚本自身能力 | 无文件系统 / shell；读写与执行全部通过 agent |
| 并发 | 最多 16 个并发 agent（CPU 少的机器更少） |
| 总量 | 单次 run 最多 1000 个 agent |
| 恢复 | 同会话内 resume：已完成 agent 返回缓存结果，运行中 agent 重跑；退出会话则全新开始 |

## 6. 进度与管理

- `/workflows` 列出 run，选中看分阶段视图（每阶段 agent 数、token、耗时），可下钻到单个 agent 的 prompt / 工具调用 / 结果
- 按键: `p` 暂停 / 恢复、`x` 停止、`r` 重启单个 agent、`s` 保存为命令、`f` 按状态过滤
- 任务面板有一行式进度摘要

## 7. 成本护栏

- workflow 显著放大 token 消耗，计入订阅用量与速率限制
- 大 run 警告: >25 agent 或预估 >1.5M token 时任务面板显示警告（v2.1.203+，仅提示不阻断）
- `/config` 的 Dynamic workflow size 可设默认规模指引: unrestricted / small(<5) / medium(<15) / large(<50)
- 所有 agent 默认用会话模型，脚本可给阶段指定别的模型，`CLAUDE_CODE_SUBAGENT_MODEL` 环境变量可整体覆盖

## 8. kxen 的取舍

直接吸收：

- 脚本化编排 + 后台 runtime + 中间结果留在脚本变量
- agent 级结果缓存 -> 会话内 resume
- 分阶段进度视图与单 agent 下钻
- 保存为可复用命令（项目级 / 个人级双位置）
- 规模指引与大 run 警告

kxen 的不同点（也是卖点）：

- Claude 的 16 并发 / 1000 总量是固定硬编码；kxen 的并发、速率、配额、预算全部挂在全局 Model Resource Manager 上，按 provider / 角色动态调度（见 `design/02-model-routing.md`）
- `agent()` 原生支持角色参数，不同阶段自动路由到不同订阅的模型（Claude 官方只能整体覆盖一个 subagent 模型）
- 脚本 API 增加 `constraints()` 查询，让写脚本的 AI 能主动感知当前余量再决定 fan-out 规模
- 与 Goal 系统打通：workflow 可作为 goal 的执行单元，结果回流做 completion 验证
