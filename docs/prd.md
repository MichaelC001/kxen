# kxen 产品需求文档 (PRD)

- 版本: 1.1
- 日期: 2026-07-20
- 状态: 已对齐，外部事实已经过实搜核实（核实结果见 `research/` 目录）

## 1. 产品概述

- 产品名: kxen
- 域名: https://kxen.ai （已注册）

kxen 是一个开源的终端原生 Coding Agent Harness，**只做 coding 场景**（软件工程任务，不做通用助手）。目标是融合 Claude Code 与 Kimi Code 两家最优秀的 workflow 模式，提供一流的多模型编排能力和明确的资源治理，面向已经持有多个模型订阅（Claude / Codex / Grok / Kimi）的重度用户：一个 harness 混用全部订阅，并且可调度、可控制、可魔改。

## 2. 核心目标

- 优先复用已有的付费订阅（订阅 / OAuth 路径），而不是纯按 token 计费的 API key
- 支持规模化的多 agent 编排（sub-agent 并行 + 脚本化 workflow）
- 同时提供高层自治目标执行（Goal）和低层可控编排（Dynamic Workflow）
- 系统高度可修改、可扩展（Pi / OMP 哲学）
- 保证执行性能与清晰的资源治理（全局统一的模型资源调度）

## 3. 关键需求

### 3.1 多模型与订阅支持

原生支持四大编码订阅，可行路径已全部验证（细节与风险分级见 `research/03-subscription-auth.md`）：

- Claude (Anthropic Pro / Max): OAuth 可行，但 Anthropic ToS 禁止在官方客户端之外使用订阅 token，属于高风险路径，需明确告知用户
- Codex (ChatGPT Plus / Pro): OpenAI 默许第三方接入（OpenCode 官方零配置支持），风险低
- Grok (SuperGrok / X Premium+): xAI 官方宣布支持 OpenCode 接入，OAuth + device code 双流程，风险低（部分档位有 403 个案）
- Kimi (Kimi Code 会员): Moonshot 官方支持第三方工具，订阅后发放 API Key 走会员配额，OpenAI / Anthropic 双协议兼容，风险低

角色化模型路由：系统必须支持定义专门角色，不同阶段 / sub-agent 自动路由到对应角色的模型：

- thinking: 强思考 / 分析模型
- planning: 任务规划模型
- execution: 高速执行模型
- review: 审查 / 对抗验证模型
- research: 研究 / 搜索模型
- tiny: 轻量后台任务（标题生成、记忆整理等）

### 3.2 Claude Code 风格 Dynamic Workflows

- 编排逻辑代码化：agent 生成（或用户编写）编排脚本，由 runtime 后台执行；脚本持有循环、分支、并行 fan-out、pipeline 与中间结果
- 主会话上下文只接收最终结果，中间过程不污染
- 关键能力：后台执行、分阶段进度、并行 fan-out 与 pipeline、会话内可恢复、可保存为可复用命令
- 支持对抗审查、多角度规划、迭代修复直到检查通过等质量模式
- 适用大规模任务：全库审计、批量迁移、多源交叉研究等
- 机制细节见 `research/04-claude-workflows.md`

### 3.3 Kimi Code 风格 Goal 系统

- 一等公民 Goal 系统（对标 kimi-code 的 `/goal` 生命周期）
- 完整状态机: create -> active -> pause / resume -> complete / blocked / budget_limited
- Goal 支持：明确目标与验证标准（completion contract）、token / agent 预算、多目标排队、自动续跑直到验证通过或预算耗尽
- Goal 可触发或拆解为 Dynamic Workflow / 并行 sub-agent
- 机制细节见 `research/05-kimi-goal.md`

### 3.4 全局 Model Resource Manager

资源约束是全局的，作用于一切多 agent 行为（直接 sub-agent 调用、Goal 并行、Dynamic Workflow 等），不与任何单一功能绑定。统一管理层负责追踪并强制执行：

- 每 provider 并发上限
- 每 provider 速率限制（requests / tokens）
- 订阅配额感知（在可探测的范围内，best-effort）
- 每角色并发上限
- 全局并发 agent 上限
- 会话级 / Goal 级 token 与成本预算

关键行为：

- 所有模型调用与 sub-agent spawn 都必须经过 acquire -> use -> release
- 首选 provider 受限时自动降级到 fallback 链
- 达到上限时排队而不是报错
- 实时状态对规划 / 编排 AI 可见（注入上下文 + 可查询工具），让 AI 主动做调度决策
- 优雅降级，避免级联失败

### 3.5 可扩展性与架构倾向

- 主要实现语言: TypeScript
- 运行时与打包: Bun（开发体验 + `bun build --compile` 单文件跨平台分发，已验证可行）
- 第一阶段纯 TypeScript，最大化魔改速度
- 架构预留原生扩展点（Rust / N-API），后续只对真正的热点路径加速（搜索、AST、隔离等），参考 OMP 的分层
- 强扩展系统（extensions / skills / hooks / commands / packages，Pi 风格）
- 清晰分层：core agent loop、模型资源管理、Goal 系统、workflow runtime、sub-agent 隔离与通信

### 3.6 性能与运行预期

- 合理的启动时间与内存占用
- 并发 sub-agent 下的稳定表现
- 资源压力下的确定性行为（排队 / 降级可预期）
- 同时支持交互式会话与长时间自治运行

### 3.7 安全模型（已决策）

原则：灾难防护走执行层机械拦截，内容不做风控。

- 硬性保护（执行层强制，不可被提示词绕过）：灾难操作机器直接拒绝，代表类包括毁系统（`rm -rf /`、写系统目录、抹除磁盘）、毁用户目录（`$HOME` 及 `~/.ssh` 等关键配置）、删 git 仓库（`.git` 与仓库根）、毁数据与基础设施（`DROP DATABASE`、`terraform destroy`、云资源删除）等；完整规则集与 approval 档边界见 `design/05-safety-rules.md`
- 实现位置：权限规则引擎（forbidden 档）+ 命令解析 + 路径守卫 + 审计三重拦截，全部在代码层，不依赖模型自觉
- 内容类话题（逆向、破解、外挂等 dual-use 场景）：kxen 不做提示词级拦截与风控，系统提示词中不内置拒绝清单；provider 侧自带的对齐策略不在 kxen 干预范围

## 4. 非目标（首版不做）

- 完整 IDE 替代品
- 云端托管 agent 平台
- 第一天支持所有模型 provider
- 对所有 provider 的完美实时配额追踪（best-effort 即可）

## 5. 成功标准

- 持有 Claude + Codex + Grok + Kimi 订阅的用户能在一个 harness 里全部用起来
- 复杂多步任务既能表达为 Goal，也能表达为 Dynamic Workflow
- 系统尊重各 provider 限制，受限时优雅降级而不是级联失败
- 代码库保持团队可理解、可扩展、可魔改
- 核心 workflow（Goal 生命周期 + 多模型 sub-agent 编排 + Dynamic Workflow）体验上是一体的，不是拼凑的

## 6. 风险与开放问题

- Claude 订阅复用有明确的 ToS 风险（Anthropic 禁止官方客户端外使用），产品需提供风险提示与官方 OAuth / 凭证复用两种模式的取舍（见 `research/03-subscription-auth.md`）
- 各 provider 订阅配额的探测与刷新策略（目前只有 Kimi 有明确的 `/usage` 与 quota 概念，其余靠限流信号被动感知）
- sub-agent 隔离机制选型（worktree / 进程 / 轻量容器；OMP 的 pi-iso 支持 APFS / btrfs / overlayfs，可参考）
- Dynamic Workflow 脚本在不同权限模式下的自动生成 vs 人工审批策略
- 底座选型待最终确认：以 Pi 的 SDK 包为底座自研编排层（推荐），还是 fork OMP 后裁剪（见 `design/01-architecture.md`）
