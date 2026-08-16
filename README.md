# kxen

macOS、Windows 和 Linux 上的 Coding Agent 工作台，也可以用浏览器访问。把多模型供应商、目标驱动的任务执行、动态工作流、Agent 团队、独立 Bots、本地工具和长期知识组织在一个本地应用中，高风险操作在执行层统一审批。另提供独立的 `kxen-agent` autonomous CLI，可在 CI、queue worker、本地终端和轻量运行环境中直接完成非交互任务，不依赖 `kxen` server。

官网与产品文档: [https://kxen.ai](https://kxen.ai)

## 下载

在 [GitHub Releases](https://github.com/StringKe/kxen/releases/latest) 下载最新版本，覆盖 macOS、Windows、Linux 的桌面应用、`kxen-<platform>` server asset 和独立的 `kxen-agent-<platform>` CLI asset；它们参与同一个版本发布。kxen server 还以多架构镜像发布在 `ghcr.io/stringke/kxen`。安装说明、签名状态和验签方法见官网。

当前为开发预览版本。

## 主要能力

- **Workspace 与 Session**: 以本地项目为边界组织会话、配置和执行状态；对话可形成持久分支树，编辑重发和重新生成保留原时间线，中断后原子续跑，存储损坏可恢复。Trajectory 检视视图把会话事件流投影为按轮次组织的记录表，附 Overview 时间线与单条记录检查器，用于核对模型每步实际看到的上下文、token 用量与耗时。
- **多模型**: 46 个内置 Provider 条目（含 AWS Bedrock 与 Google Vertex AI）、多账号管理、订阅 OAuth 和 API key 登录，按角色路由模型与降级；自定义端点支持 OpenAI 与 Anthropic 兼容协议及 query 参数（可接 Azure OpenAI）。
- **目标与编排**: Goal、Subagent、Dynamic Workflow、Agent Teams 和 Kanban 流水线，后台任务完成逐路回执。
- **Bots**: 独立、可版本化的重复工作单元。每个 Bot 都能通过自己的受限 self-builder 对话创建和完善定义，并支持 Routine、可恢复 BotRun，以及 Direct 和 2 至 6 Bot Group 的 Bot-to-Bot 协作。
- **DCP**: Deterministic Context Pipeline 以 durable facts 重建 Provider-neutral context，并为 turn、tool 副作用、`UNKNOWN` 和 settlement 提供一致边界。
- **kxen-agent CLI**: 从 task 动态构建或加载 DCPAgent YAML，使用 immutable capability/policy lock 执行完整任务；支持 durable DCPRun、`--resume`、Conversation branch、Git worktree、跨 runner bundle 和 UNKNOWN tool recovery。仓库同时提供 verifier -> fixer -> reviewer -> draft PR 的 GitHub Issue reference workflow；GitHub/GitLab 等平台能力不进入 DCPAgent definition。
- **本地工具**: 文件、Shell、Web Fetch、Web Search、Browser、MCP 和 LSP。`workflow` 沙箱内模型可用 `tool()` 在一次调用中编排多步工具执行；`tool_define` 让模型经审批定义会话级动态工具（`tool_undefine` 同口径卸载），DCP 侧以 `allowDynamicTools` policy 开关和宏目录提供提案-审批-新会话生效的路径。
- **长期知识**: OKF v0.2 Knowledge Library、Rules、Skills、Memory、generic concepts 和自动沉淀。
- **安全边界**: 执行层 Safety 与 Approval、可持久化的会话级审批规则与审批审计、Checkpoint、可撤销的 Rewind、Worktree 隔离与合回，文件删除只进废纸篓。
- **日常效率**: Voice、Schedule、Usage 统计、通知和诊断。

Bot Group 的成员都是 Bot，不是多人真人聊天。kxen 不提供共享云电脑、共享浏览器凭据或 Bot Marketplace。完整使用说明见 [Bots 文档](https://kxen.ai/bots/)。

对话分支隔离 Session 历史、队列、运行和草稿，但不会复制 Workspace 文件。需要隔离文件实验时使用独立 Worktree；需要回退文件和对话时使用 Rewind。完整语义见 [Session 文档](https://kxen.ai/workspace/session/)。

## OKF 长期知识

项目知识位于 `file:///path/to/workspace/.agents/`，个人知识位于 `file:///Users/you/.agents/`。目录层级只负责组织，每个非 reserved Markdown concept 都使用可解析的 YAML frontmatter 声明非空 `type`:

```md
---
type: refactor
title: Safe refactoring
description: Constraints and verification for structural code changes.
tags: [code, rust]
---
```

Kxen 为 `rule`、`reference`、`skill`、`command`、`note`、`memory` 和 `history` 提供不同运行 handler。`code`、`refactor`、`test` 等自定义 type 会作为 generic concept 被索引和检索，不会获得可执行语义。`index.md` 用于渐进披露，`log.md` 用于历史记录，根 `index.md` 可以声明 `okf_version: "0.2"`。

检索使用当前 user task 和涉及文件作为 query。Notes 与 Memory 通过 description 和正文做 BM25；generic、reference 和 history concepts 还使用 type、路径、title、tags，并沿本地 Markdown links 做一跳扩展。可选 embedding 按 endpoint、provider、model 和内容 hash 增量缓存。检索热路径不等待网络，embedding 缺失或失败时回退 BM25。当前本地规模不需要单独部署向量数据库。完整格式、handler 和信任边界见 [Knowledge 文档](https://kxen.ai/knowledge/knowledge-library)。

## 独立 agent CLI

`kxen-agent` 直接接收 task，不需要启动 `kxen` server。省略 `--agent` 时会先用受限 Builder 创建 DCPAgent definition；传入 YAML 时使用可审计的预定义 agent:

```bash
cargo build --release -p kxen-agent

./target/release/kxen-agent run \
  --workspace /path/to/repository \
  --agent examples/kxen-agent/repository-fixer.dcpagent.yaml \
  --policy examples/kxen-agent/policy.json \
  --task "定位问题、完成修复并运行相关验证"
```

默认输出 JSONL。Session 和 DCPRun 会持久化，可以用 `kxen-agent --resume SESSION_ID` 恢复，或使用 `session fork/export/import` 在对话分支、Git worktree 和 ephemeral runner 之间迁移。完整执行契约、权限与自动化示例见 [kxen-agent 文档](https://kxen.ai/agent-cli/)。

### GitHub Issue 自动修复

`.github/workflows/kxen-issue-autofix.yml` 是一个完整场景实现，不是 DCP 核心协议。可信的 `gh` step 先读取一个 Issue，再把结构化数据交给没有 GitHub token、Shell 或 MCP 的 context DCPAgent；fixer 修改 Workspace，reviewer 在独立 checkout 中验证。只有结构化结果和 deterministic diff gate 全部 PASS 后，全新的可信 runner 才从校验过的 text patch 重建变更并创建 topic branch、draft PR 和 Issue comment。

启用时在 `agent-automation` GitHub Environment 配置 `XAI_API_KEY` secret，以及 `XAI_MODEL`、`KXEN_AGENT_VERSION` variables。模型 jobs 在独立 job containers 中运行，通过可信 step 创建 private one-shot auth file，`kxen-agent` 在任何 tool subprocess 启动前消费并 unlink；模型 runner 不获得 GitHub credential。具有 write 权限的维护者给 `bug` Issue 添加 `kxen:fix` label 后开始执行。definitions、policies、凭据边界和恢复方法见 [自动化与 GitHub 场景](https://kxen.ai/agent-cli/automation/)。

## DCP

DCP 是 Deterministic Context Pipeline，不是网络 transport 或平台 adapter 协议。它要求先持久化可验证事实，再确定性投影为有序、Provider-neutral 的模型上下文；模型输出和工具结果在进入下一次重建前形成 durable record。已经跨过副作用边界却无法证明结果的 operation 保持 `UNKNOWN`，不会自动重放。

BotRun 直接使用 DCP `ContextFrame`、`ProviderNeutralPart`、`TurnRecord` 和 tool boundary journal；`kxen-agent` 使用 DCPAgent lock、DCPRun、Workspace binding、resume 与 bundle。Kanban 的列 Agent 有独立 definition，不是 DCPAgent YAML。GitHub、GitLab、Issue、PR、branch 和 comment 由 MCP、CLI、内置 tool 或宿主调用提供，不进入 DCP 核心模型。完整说明见 [DCP 文档](https://kxen.ai/concepts/dcp)。

## 开发

```bash
pnpm install
pnpm tauri:dev
```

验证:

```bash
pnpm check
pnpm test
cargo test --workspace
cargo build --release -p kxen-agent
```

官网源码在 `website/`。发布流程与贡献规范见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 许可

[MIT](LICENSE)
