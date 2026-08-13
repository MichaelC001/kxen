# kxen

macOS、Windows 和 Linux 上的 Coding Agent 工作台，也可以用浏览器访问。把多模型供应商、目标驱动的任务执行、动态工作流、Agent 团队、独立 Bots、本地工具和长期知识组织在一个本地应用中，高风险操作在执行层统一审批。

官网与产品文档: [https://kxen.ai](https://kxen.ai)

## 下载

在 [GitHub Releases](https://github.com/StringKe/kxen/releases/latest) 下载最新版本，覆盖 macOS、Windows、Linux 的桌面应用和可无头运行的 kxen server（浏览器访问，含 tailscale 远程场景）；kxen server 同时以多架构镜像发布在 `ghcr.io/stringke/kxen`。安装说明、签名状态和验签方法见官网。

当前为开发预览版本。

## 主要能力

- **Workspace 与 Session**: 以本地项目为边界组织会话、配置和执行状态；对话可形成持久分支树，编辑重发和重新生成保留原时间线，中断后原子续跑，存储损坏可恢复。
- **多模型**: 44 个内置 Provider 条目、多账号管理、订阅 OAuth 和 API key 登录，按角色路由模型与降级。
- **目标与编排**: Goal、Subagent、Dynamic Workflow、Agent Teams 和 Kanban 流水线，后台任务完成逐路回执。
- **Bots**: 独立、可版本化的重复工作单元。每个 Bot 都能通过自己的受限 self-builder 对话创建和完善定义，并支持 Routine、可恢复 BotRun，以及 Direct 和 2 至 6 Bot Group 的 Bot-to-Bot 协作。
- **本地工具**: 文件、Shell、Web Fetch、Web Search、Browser、MCP 和 LSP。
- **长期知识**: Rules、Skills、Memory 和自动沉淀的 Knowledge Library。
- **安全边界**: 执行层 Safety 与 Approval、Checkpoint、Rewind、Worktree 隔离，文件删除只进废纸篓。
- **日常效率**: Voice、Schedule、Usage 统计、通知和诊断。

Bot Group 的成员都是 Bot，不是多人真人聊天。kxen 不提供共享云电脑、共享浏览器凭据或 Bot Marketplace。完整使用说明见 [Bots 文档](https://kxen.ai/bots/)。

对话分支隔离 Session 历史、队列、运行和草稿，但不会复制 Workspace 文件。需要隔离文件实验时使用独立 Worktree；需要回退文件和对话时使用 Rewind。完整语义见 [Session 文档](https://kxen.ai/workspace/session/)。

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
```

官网源码在 `website/`。发布流程与贡献规范见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 许可

[MIT](LICENSE)
