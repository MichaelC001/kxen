# 底座选型复盘：pi vs OpenCode vs grok-build

- 日期: 2026-07-20
- 背景: pi 集成过程中出现多次「魔改感」问题，评估是否换底座
- 结论先行: **不全盘换。用 1 天 spike 验证 OpenCode SDK 作为新底座；grok-build 直接排除（Rust + 1M 行 + 不收 PR）；若 spike 不达标则降级为方案 D（只用 pi-ai / pi-agent-core / pi-tui，自写 CLI 层）**

## 1. 本会话 pi 踩坑实录（成本证据）

| 坑 | 根因层 | 性质 |
| --- | --- | --- |
| AuthStorage 更名为 CredentialStore，老文档失效 | pi-coding-agent API | 高 churn（270 个版本） |
| isolated store 下 import 解析失败（./main 无 subpath export） | 包发布形态 | 集成摩擦 |
| APP_NAME 换牌：PI_PACKAGE_DIR 级联到 themes / docs / examples / 模板，被迫建 bridge 符号链接目录 | pi-coding-agent CLI 层假设（它是「pi 这个 CLI」，不是「你的 CLI」） | 架构错位 |
| 交互层资产（主题 dark.json 等）按 packageDir 相对查找 | 同上 | 架构错位 |
| 代理版号 0.80.x 周更，接口仍动 | 全包 | 高 churn |
| 我们被迫写 KxenSession 包装层 + 各处胶水 | 同上 | 我们自己的复杂度 |

但同时必须承认：**pi-ai 与 pi-agent-core 全天零故障**——40+ provider、四个订阅 OAuth、agent loop、sessions、usage 统计全部一次通过。痛苦集中在 pi-coding-agent 的 CLI 层（main()、InteractiveMode 资产、APP_NAME）。

## 2. 三个候选的「被嵌入」能力（已核实）

| 维度 | pi (@earendil-works) | OpenCode (@opencode-ai/sdk) | grok-build |
| --- | --- | --- | --- |
| 嵌入形态 | 进程内 SDK（createAgentSession / main / InteractiveMode） | 类型安全 SDK：createOpencode（server+client）/ createOpencodeServer / createOpencodeClient，OpenAPI + SSE | 库形态为零，只有 CLI 源码（自己 fork） |
| 订阅认证 | Claude Pro/Max OAuth 内置；其余靠自家 auth.json | 官方全支持：Claude Pro/Max、ChatGPT Plus/Pro、SuperGrok/X Premium、GitHub Copilot、GitLab Duo | SuperGrok 原生 |
| Agent 模型 | 极简（read/write/edit/bash + 扩展），不内置 plan/subagent，适合当底座 | build/plan primary + 内置 subagent，权限模型完整但与我们的 goal / workflow / MRM 语义不同 | 完整（plan mode / 并行 subagent / worktree） |
| 语言 / 栈 | TS (Node/Bun) | TS (Bun + Hono) | Rust（约 1M 行，不收 PR） |
| 与我们设计冲突点 | CLI 层「我是 pi」的假设 | client-server 重量、agent 模型已 baked-in、无 goal / dynamic workflow 概念 | 语言违反 TS 决策、无人可协作 |
| 社区 / 维护 | 73k stars，单作者高速迭代 | 极活跃（18x k 量级），公司化维护 | 官方 monorepo 同步 |

## 3. 关键认知：我们的核心资产不依赖 pi

MRM / 角色路由 / goal 引擎 / dynamic workflow runtime / subagent typed 结果 / hooks / DCP compose / 检查点——全部是自研包，与 pi 只有三条接缝：

1. 会话与模型调用（createAgentSession / ModelRegistry）
2. 工具定义形态（ToolDefinition + typebox）
3. 交互层（InteractiveMode / TUI）

换底座只动这三条缝。goal / workflow / mrm / resources / router / context / providers（凭证导入逻辑）原样保留。

## 4. 方案对比

| 方案 | 内容 | 工作量 | 风险 |
| --- | --- | --- | --- |
| A. 维持 pi 不动 | 修当前 -p 小 bug（在我们包装层，非 pi），保留 bridge | 小（小时级） | 继续吃 churn 与 CLI 层摩擦 |
| B. 迁 OpenCode | 以 opencode server + SDK 为底座：会话 / auth / tools 接缝重写，核心包保留 | 中（2-4 天） | client-server 重量；其 agent 模型与我们的 goal / workflow 语义冲突（要 subvert 它的编排来达成我们的） |
| C. 迁 grok-build | fork 1M 行 Rust | 大 | 违反 TS 决策、无 PR、孤立维护，**直接排除** |
| D. 降级只用 pi-ai / pi-agent-core / pi-tui | 丢掉 pi-coding-agent 整个 CLI 层（APP_NAME / assets / bridge 全消失），自写 100 行级 main + TUI | 中（1-2 天） | 自写会话交互层（pi-agent-core 的 Agent 类就是为此设计的） |

## 5. 结论

1. **不重写核心**。换底座只动三条接缝，不存在「全部推翻」的选项
2. **Grok Build 排除**：Rust + 1M 行 + 不收外部 PR，与我们全部既定决策冲突
3. **OpenCode 值得一个 1 天 spike**：它确实在订阅认证与 SDK 设计上比 pi 干净（官方四订阅 + OpenAPI + Bun），若接缝成本低则迁；但它也是「别人的 CLI / agent 模型」，goal / dynamic workflow / MRM 还是要我们自己写——底座只解决会话、认证、工具、会话持久化
4. **spike 不达标就执行 D**：pi-ai / pi-agent-core / pi-tui 继续用（全天零故障的部分），pi-coding-agent 丢掉，自写 CLI 层。这是确定能成的退路
5. spike 验收标准：用 OpenCode SDK 完成（a）四订阅登录可用、（b）一个带我们工具的会话跑通、（c）MRM 能包在它的模型调用外层——三条全过才算可迁

## 6. 立即执行

- spike 分支跑 4.5 的三条验收
- 期间主线修掉当前 -p 包装层 bug（方案 A 的保底，成本极低）
- 评审会拍板：OpenCode 底座 / D 降级底座
