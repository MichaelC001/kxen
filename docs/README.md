# kxen 文档索引

kxen 是一个开源的终端原生 Coding Agent Harness：混用 Claude / Codex / Grok / Kimi 四个订阅，融合 Claude Code 的 Dynamic Workflow 与 Kimi Code 的 Goal 生命周期，自带全局模型资源调度。域名 https://kxen.ai （已注册）。

## 阅读顺序

1. `prd.md` - 产品需求文档（定位、目标、需求、成功标准、开放问题）
2. `research/` - 外部事实调研（2026-07-20 实搜核实，含来源）
3. `analysis/` - 核心维度深度分析（context 工程、工具面、模型调度）
4. `design/` - 内部设计决策

## research/

| 文件 | 内容 |
| --- | --- |
| `01-cli-agent-landscape.md` | 开源 CLI agent 全景、语言与性能对比、选型结论（Pi 底座 + 自研编排层） |
| `02-pi-and-omp.md` | Pi 与 OMP 深度调研：架构、包生态、角色路由、subagent、可借鉴清单 |
| `03-subscription-auth.md` | 四大订阅的第三方接入路径、官方态度、风险分级、配额可探测性 |
| `04-claude-workflows.md` | Claude Code Dynamic Workflows 机制全解 + kxen 的取舍 |
| `05-kimi-goal.md` | Kimi Code Goal 生命周期（一手语义）+ kxen 的扩展点 |

## analysis/

| 文件 | 内容 |
| --- | --- |
| `01-context-engineering.md` | context 工程三原语、各家压缩 / 记忆 / 提醒机制对比、kxen 决策 C1-C9 |
| `02-tool-surface.md` | 命令粒度、误用纠偏、输出减载、专用工具不变量、渐进披露、权限规则、kxen 决策 T1-T12 |
| `03-provider-scheduling.md` | 各 provider 限流信号、限流机制库（bucket / AIMD / 熔断 / 队列）、kxen MRM 定稿算法 |
| `04-design-synthesis.md` | 优点收纳矩阵：每个维度的最佳实践来源与 kxen 选型定稿、三条主线、反模式清单 |
| `05-dcp-subagents.md` | DCP (Deterministic Context Pipeline) 调研：IR 中立存储、compose 规则、probe gate、cursor 压缩对子代理的适用性（含源码级参数） |
| `06-engineering-experience.md` | 工程化体验：内存预算 E1-E8（CC / OpenCode 事故教训）、性能、mermaid 纯 Rust 渲染决策、可观测性 |
| `07-system-prompt.md` | 系统提示词：逆向资源清单、CC / grok-build / Codex 结构对比、六个注入层、kxen 组装设计 P1-P11 |
| `08-hooks-statusline.md` | Hooks（CC 30 事件协议）与 statusline 机制全解、kxen 全面管控 + 可选开启设计 |
| `09-exec-lsp-mcp.md` | exec 多 shell 工具（X1-X6）、原生 LSP + auto-detect（L1-L6）、MCP 自动探测与渐进披露 |

## design/

| 文件 | 内容 |
| --- | --- |
| `01-architecture.md` | 底座选型（Pi SDK 方案 B）、分层架构、模块边界、里程碑 |
| `02-model-routing.md` | 角色化模型路由 + 全局 Model Resource Manager（并发 / 速率 / 配额 / 预算） |
| `03-goal-and-workflow.md` | Goal 状态机、Workflow runtime 脚本模型、二者联动流程 |
| `04-tech-stack.md` | Bun + TypeScript 决策、单文件分发、性能分层（TS -> 按需 Rust N-API） |
| `05-safety-rules.md` | 灾难操作防护规则集：forbidden / approval 分档、F1-F5 规则族、防绕过三重拦截 |
| `06-monorepo.md` | monorepo 决策：bun workspaces + catalog（非 pnpm）、layout、版本策略、工具链 |
| `07-subagents.md` | 子代理能力设计：kimi-code swarm + claude-code sub-agents + OMP + OpenDev 收敛 |
| `08-config-interop.md` | 配置互通：std-agent 需求还原、canonical 格式、kxen import、Agent Skills 对齐 |
| `09-agents-directory.md` | `.agents/` + AGENTS.md 规范：rules / references 类型路由、OKF 引入、多层目录解析 |
| `06-monorepo.md` | monorepo 工程结构：bun workspaces + catalog 决策、包 layout 映射、版本与发布策略、工具链、crates/ 预留 |

## 当前状态

- 已落地: 调研（research/）、核心维度分析（analysis/）、PRD v1.1、架构与设计初稿（design/）
- 进行中: 设计评审
- 未开始: M0 骨架（Bun 工程 + pi SDK 接入）

## 事实核实说明

本文档库中所有外部事实（仓库、版本、包名、官方政策、价格、API 端点）均于 2026-07-20 通过实搜与 registry 直查核实，来源以 https:// 链接列在各文档内。后续如官方政策变化（尤其是 Anthropic 对第三方使用 Claude 订阅的态度），以最新官方页面为准。
