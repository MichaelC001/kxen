# kxen

macOS、Windows 和 Linux 上的 Coding Agent 工作台，也可以用浏览器访问。把多模型供应商、目标驱动的任务执行、动态工作流、Agent 团队、本地工具和长期知识组织在一个本地应用中，高风险操作在执行层统一审批。

官网与产品文档: [https://kxen.ai](https://kxen.ai)

## 下载

在 [GitHub Releases](https://github.com/StringKe/kxen/releases/latest) 下载最新版本，稳定 asset 命名保证同一链接始终解析到最新版本:

- macOS(Apple Silicon、Intel): DMG，经过 Developer ID 签名和 Apple 公证，打开后把 `Kxen.app` 拖入「应用程序」，需要 macOS 14 及以上版本。
- Windows(x64、ARM64): NSIS 安装包，暂无 Authenticode 签名，SmartScreen 提示时选择 More info -> Run anyway。
- Linux(x86_64、ARM64): AppImage 与 deb。
- kxen-web 无头 server(六个平台的 tar.gz/zip): 启动后打印带 token 的访问 URL，浏览器（含 tailscale 远程）打开即可使用全部功能，用法见官网 Web 模式文档。

桌面应用内置自动更新（deb 除外）。每个版本附带 `SHA256SUMS` 和 updater 签名，可校验下载产物。

当前为开发预览版本。

## 主要能力

- **Workspace 与 Session**: 以本地项目为边界组织会话、配置和执行状态，中断后原子续跑，存储损坏可恢复。
- **多模型**: 44 个内置 Provider 条目、多账号管理、订阅 OAuth 和 API key 登录，按角色路由模型与降级。
- **目标与编排**: Goal、Subagent、Dynamic Workflow 和 Agent Teams，后台任务完成逐路回执。
- **本地工具**: 文件、Shell、Web Fetch、Web Search、Browser、MCP 和 LSP。
- **长期知识**: Rules、Skills、Memory 和自动沉淀的 Knowledge Library。
- **安全边界**: 执行层 Safety 与 Approval、Checkpoint、Rewind、Worktree 隔离，文件删除只进废纸篓。
- **日常效率**: Voice、Schedule、Usage 统计、通知和诊断。

## 开发

```bash
pnpm install
pnpm tauri:dev
```

验证:

```bash
pnpm check
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml
```

官网源码在 `website/`。发布流程与贡献规范见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 许可

[MIT](LICENSE)
