# 子代理能力设计（kimi-code swarm + claude-code sub-agents 收敛）

- 日期: 2026-07-20
- 定位: 把四个参照系的子代理能力收敛为 kxen 的完整子代理模型；与 analysis/04 矩阵、design/03、analysis/05 对齐

## 1. 参照系能力对照

| 能力 | claude-code sub-agents | kimi-code Agent Swarm | OMP | OpenDev (论文) |
| --- | --- | --- | --- | --- |
| 定义方式 | `.claude/agents/*.md` frontmatter（name / description / tools / model） | 内置 coder / explore / plan 三型 | task 工具 + 预置 specialist | SubAgentSpec（name / prompt / 工具白名单 / model override） |
| 上下文 | 独立窗口，父只看到最终结果 | 独立上下文，保持主会话干净 | 独立 + schema 校验的 typed 结果 | message_history=None 全新上下文 |
| 批量派发 | Agent tool 逐个 | 同模板批量（上限 128，自动排队，scope 互不冲突） | 并行 fan-out | - |
| 前后台 | foreground / background（bg 的权限提示浮到主会话，v2.1.186+） | 前后台 + 完成通知 + resume | 并行 + channel 通信 | - |
| 隔离 | 无文件级隔离 | 任务级隔离 | worktree（APFS / btrfs / overlayfs） | - |
| 工具控制 | tools 字段 + deny 规则 | 子代理类型决定 | 权限继承 + agent 定义 | schema 级过滤：看不到的工具不会误用 |
| 模型 | 跟随父或 `CLAUDE_CODE_SUBAGENT_MODEL` 覆盖 | 跟随主模型 | 按角色 + agent 定义多 pattern fallback | 按 workflow 独立绑定 |
| 纠偏 | - | 中途 steering / 停止 / 停止原因 | - | doom loop 检测 |
| hooks | SubagentStart / SubagentStop（可按 agent 类型匹配） | 任务事件 | - | - |

kxen 独有的增益（前序文档已定）：

- 每个子代理的模型按角色路由 + 全局 MRM 调度（并发 / 速率 / 配额 / 预算），不是跟随主模型也不是简单覆盖（design/02、analysis/03）
- 上下文构造走 DCP pipeline：TR 以 provider 中立 IR 持久化 + composeContext 确定性规则（analysis/05），多模型 fallback 不炸历史
- probe gate：fan-out 前由 tiny 角色做「值不值得跑」裁判（analysis/05）

## 2. kxen 子代理模型

### 定义

- 内置四型: `explore`（只读研究）、`plan`（只读规划）、`execute`（全工具执行）、`review`（只读审查）
- 自定义: `.agents/agents/*.md`（frontmatter: name / description / role / tools / model 可选覆盖），项目级优先于用户级 `~/.agents/agents/`
- 工具控制在两层: schema 级过滤（OpenDev，看不到即不会误用）+ execpolicy（运行时拦截，design/05）

### spawn 与 swarm

- `agent()`: 单个派发，foreground / background 可选；background 完成时通知，禁止轮询
- `swarm()`: 同模板批量（对齐 AgentSwarm 语义）：每个 item 一个子代理、scope 必须互不冲突、数量上限由 MRM 动态决定（不是固定 128）、自动排队
- fork 模式: 需要父上下文时继承父会话副本（claude-code fork 语义），默认关
- steering: 运行中可向子代理注入追加指令；停止时记录 stop_reason

### 结果与恢复

- typed result: 每个子代理返回 schema 校验的结构化结果（summary + artifacts + 文件变更清单 + usage），不是纯文本（OMP）
- 持久化: TR 以 IR 落盘，crash 后可按最近一致点恢复（DCP 步骤持久化规则）
- 隔离: worktree（默认）-> CoW（M5）；并发写冲突靠隔离而不是锁

### 观测

- hooks: SubagentSpawn / SubagentComplete（analysis/08），可按类型匹配
- statusline / 面板: 每个子代理一行（模型、角色、token、状态、stop_reason），对齐 CC subagentStatusLine 语义
- 事件流: spawn / steering / 完成 / 失败全部可回放

## 3. 与里程碑对齐

- M2: agent / swarm / typed result / worktree 隔离 / IR 存储 / 内置四型
- M3: goal 引擎派发子代理（goal 轮内选择直接干或派子代理）
- M4: workflow `agent()` / `pipeline()` 复用同一 spawn 协议；probe gate
- M5: CoW 隔离、自定义 agents 目录、`agent.md` 与 std-agent 生态格式对齐（见 design/08）
