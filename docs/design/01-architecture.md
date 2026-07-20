# kxen 总体架构

- 日期: 2026-07-20
- 状态: 初稿，待评审
- 前置阅读: `docs/prd.md`、`docs/research/01-cli-agent-landscape.md`、`docs/research/02-pi-and-omp.md`

## 1. 底座选型决策

候选方案：

| 方案 | 内容 | 优点 | 缺点 |
| --- | --- | --- | --- |
| A. fork OMP | 在 can1357/oh-my-pi 上裁剪改造 | 完整度现成（工具面、LSP、subagent、路由） | 代码体量大（TS 87.6% + Rust 55k 行）、意见强；goal / workflow / 资源调度仍要重写；上游跟进成本高 |
| B. Pi SDK 为底座自研（推荐） | 依赖 pi 系包（pi-ai / pi-agent-core / pi-tui / pi-coding-agent SDK），编排层全部自研 | 核心小而稳，编排层完全自主；上游以依赖升级跟进；扩展系统现成 | 工具面（LSP / 隔离 / hashline）需要自己补或借 |
| C. 从零自研 | 全部自己写 | 完全可控 | agent loop / TUI / provider 适配重复造轮子，周期长 |

决策：方案 B。

- `pi-ai` 解决多 provider 统一调用与 token / cost 统计
- `pi-agent-core` 解决 agent loop、状态、事件、消息队列
- `pi-coding-agent` 的 SDK（`createAgentSession` / `ModelRegistry` / `SessionManager` / `readStoredCredential` + pi-ai 的 `CredentialStore`）解决会话与凭证；注：0.80.10 起旧的 `AuthStorage` 概念已更名为 CredentialStore，主导出暴露 `readStoredCredential`（2026-07-20 实测）
- kxen 自研的部分：Goal 引擎、Dynamic Workflow runtime、全局 Model Resource Manager、角色路由、subagent 管理、权限模式（plan / build）、TUI 中的 goal / workflow 视图
- 风险：pi 系包仍在快速演进（2025-11 以来 270 个版本），依赖升级需锁版本 + 定期跟进

## 2. 分层架构

```
+-----------------------------------------------------------+
| 入口层    TUI | -p 单发 | RPC | ACP（后两者后置）            |
+-----------------------------------------------------------+
| 体验层    会话 / slash command / plan-build 模式切换 /      |
|           goal 视图 / workflow 进度视图                     |
+-----------------------------------------------------------+
| 编排层    Goal 引擎 | Workflow Runtime | Subagent Manager  |
+-----------------------------------------------------------+
| 调度层    Model Resource Manager（并发/速率/配额/预算）      |
|           + Role Router（角色 -> 模型 -> fallback 链）       |
+-----------------------------------------------------------+
| 模型层    pi-ai provider 适配 + Auth（四订阅 OAuth / Key）   |
+-----------------------------------------------------------+
| 工具层    read/write/edit/bash + search/AST/LSP（渐进增强）  |
+-----------------------------------------------------------+
| 扩展层    extensions / skills / packages（Pi 形态）         |
+-----------------------------------------------------------+
```

关键流向：

- 一切模型调用（主会话、subagent、workflow 阶段、goal 续跑）都经过调度层 acquire -> 模型层执行 -> release，没有旁路
- 编排层不直接碰 provider，只表达「要什么角色、多少并发、什么预算」
- 调度层把实时余量反哺给编排层（注入 planning / thinking 模型的上下文，以及 workflow 脚本的 `constraints()` API）

## 3. 模块边界

- `core/`: 会话与 agent loop 封装（基于 pi-agent-core），plan / build 权限模式
- `goal/`: 状态机、contract、queue、验证循环（见 `design/03-goal-and-workflow.md`）
- `workflow/`: 脚本解析、受限执行、agent/pipeline API、结果缓存与恢复、命令保存
- `subagent/`: spawn、隔离（先 worktree，后接 pi-iso 思路）、typed 结果、steering
- `router/`: 角色注册表、fallback 链、thinking level（见 `design/02-model-routing.md`）
- `resources/`: 并发信号量、速率窗口、配额感知、预算账户、状态导出
- `providers/`: 四订阅的 auth flow 与凭证存储（见 `research/03-subscription-auth.md`）
- `tools/`: 基础四工具起步；search / AST / LSP 渐进增强
- `ext/`: 扩展加载器（兼容 Pi extension 形态）

## 4. 权限模式（plan / build）

参考 OpenCode 与 OMP 的做法：

- `plan` 模式：edit / write / bash 默认 deny 或 ask，只允许只读工具 + research 类 subagent；用 `plan` 角色模型
- `build` 模式：全工具开放；用 `default` 角色模型
- Tab 或 `/mode` 切换；模式本身也是「角色 + 工具集 + 权限集」的三元组，用户可自定义更多模式

## 5. 里程碑（粗粒度）

1. M0 骨架：Bun 工程、pi SDK 接入、单 provider 跑通会话 + plan/build 切换；内存三层分离、fetch 强制 drain、懒初始化自骨架期就位
2. M1 调度：Role Router + Model Resource Manager（并发 / 预算），四订阅 auth 接入；进程级内存预算 + 有界事件队列
3. M2 subagent：spawn / worktree 隔离 / typed 结果 / TUI 下钻；provider 中立 IR 存储 + composeContext 规则（DCP）；subagent 内存约束 + telemetry
4. M3 Goal：状态机 + contract + 验证循环 + queue + 检查点回滚（shadow git）
5. M4 Workflow：脚本 runtime + 缓存恢复 + 进度视图 + 保存命令 + probe gate（DCP）
6. M5 打磨：snapcompact、TTSR、hashline、CoW 隔离、离线记忆管线、mermaid、配额感知增强、性能热点评估（按需 Rust N-API）、发布管线
