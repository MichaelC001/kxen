# 配置互通策略（由 std-agent 反推）

- 日期: 2026-07-20
- 背景: std-agent（https://github.com/StringKe/std-agent ，Go CLI）以 `.stdai/` 为单一事实源，把 rules / skills / commands / references / subagents / MCP fan-out 到 23 个 AI 工具的原生格式，带漂移检测与 context budget 检查

## 1. std-agent 为什么存在（需求还原)

用户同时在用十几个 AI 编码工具，每个都有自己的配置体系（CLAUDE.md / AGENTS.md / .cursor/rules / .windsurf / .clinerules / copilot-instructions ...）。手写维护多份必然漂移；MCP 配置同样散落各处；且各家 context budget 不同，同一份规则塞给谁都可能超限。std-agent 的解法是「写一次，按各工具的格式方言分发 + 漂移检测 + 预算校验」。

## 2. kxen 面对的同一需求

kxen 的目标是一个 harness 取代多开，但用户的既有配置生态不会一夜迁移：

- 用户仓库里已经躺着 CLAUDE.md / AGENTS.md / .cursor/rules / .codex 等资产
- 团队里仍有人用别的工具，配置必须共存而不是互毁
- kxen 自己也需要一套 canonical 配置格式，不能发明第 24 种方言后袖手旁观

## 3. kxen 决策（K1-K6）

| # | 决策 | 说明 |
| --- | --- | --- |
| K1 | canonical 输入 = AGENTS.md 分层 + `.agents/` 目录 | AGENTS.md（全局 -> 项目 -> 子目录按需）是跨工具事实标准，直接继承；`.agents/` 放规则与知识（rules / references / skills / commands / agents / workflows），组织规范见 design/09；`.kxen/` 只留运行时状态 |
| K2 | `kxen import` 迁移命令 | 扫描现有 .claude / .cursor / .codex / .gemini / .windsurf / .clinerules / .github/copilot-instructions 等，盘点后按用户确认搬进 `.agents/`（OKF 文档，type 归类）与 AGENTS.md；原文不「优化」（命令、端点、路径逐字保留），与 std-agent 迁移 prompt 同原则；一次性动作，不是持续同步 |
| K3 | 一切配置文档统一为 OKF | rules / references / skills / commands / agents / workflows 全部只是 `.agents/` bundle 里的 OKF 概念文档，`type` 字段路由加载语义（design/09）；不存在 per-tool 格式适配层，std-agent 生态产出零转换可读 |
| K4 | MCP 只用通用 `mcp.json` | 与 Claude Code / Cursor / VS Code 同格式（analysis/09 MCP5），不做私有 schema；`.codex/config.toml` 只读导入 |
| K5 | kxen 作为 std-agent target 的接口约定 | 在 docs 公开 kxen 的配置目录与 frontmatter 方言（即本文档 + design/07 + analysis/07），方便 std-agent 增加 kxen target；kxen 侧保证向后兼容自己文档化的格式 |
| K6 | 不做反向 fan-out | kxen 不替其他工具写配置文件（那是 std-agent 的职责）；需要多工具同步的场景推荐直接用 std-agent |

## 4. 边界

- `.agents/` 与 AGENTS.md 的分工：AGENTS.md 放「任何 agent 都该懂的项目规则」；`.agents/` 放规则与知识的结构化目录（含编排层特有的 roles、workflows、agents 定义，规范见 design/09）；`.kxen/` 放运行时状态
- 漂移检测不做：kxen 不监控其他工具生成的文件（不越界）；`kxen import` 是可重复执行的显式命令
- context budget 检查吸收进 prompt composer（P1-P11）：注入前估算各段 token，超预算按优先级裁剪并报告

## 5. OKF 消解转换层（关键边界）

std-agent 的复杂度不在文档类型，而在 23 个消费方言（CLAUDE.md / .mdc / 数字前缀 clinerules / TOML ...）。kxen 引入 OKF 后：

- kxen 内部零转换层：`.agents/` 就是运行时直接消费的格式，rules / references 等全部只是 OKF 的 type 取值，加载语义是消费者策略（design/09 矩阵），不是格式转换
- 不需要 std-agent 的 6 个 protocol adapter 中的任何一个：那些 adapter 的存在理由是「别的工具不说 OKF」，kxen 自己原生说 OKF
- 剩下唯一与格式有关的工作是一次性迁移（K2，把别家方言搬进 OKF）和对外声明（K5，让 std-agent 把 `.agents/` 当 source 或 target）；两者都不是持续转换
