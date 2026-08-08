# Changelog

本项目的显著变更记录在此文件中。

格式遵循 https://keepachangelog.com/zh-CN/1.1.0/ ，版本遵循 https://semver.org/lang/zh-CN/ 。

## [Unreleased]

### Added

- kxen server 的 Docker 多架构镜像（linux/amd64、linux/arm64）随每次 release 发布到 `ghcr.io/stringke/kxen`，默认以 Web 模式启动，数据持久化在 `/data` 卷。

## [0.1.2]

### Changed

- 仓库结构：Cargo workspace 上移至仓库根，产品逻辑独立为 `crates/kxen-core` 库 crate，无头 server 为 `crates/kxen-cli`（产物仍名 `kxen`），`src-tauri` 仅保留 Tauri 桌面壳。

### Fixed

- Linux ARM64 发布构建：补齐 `xdg-utils`（deb/AppImage bundler 调用 `xdg-open`），此前该镜像不自带导致打包失败。

## [0.1.1]

### Fixed

- 官网生产依赖审计：`postcss` 链的 `nanoid` 高危告警（size 为 0 时死循环），通过 pnpm override 锁定 `>=3.3.17`。

## [0.1.0]

### Added

- 无头 Web 模式：新增 `kxen` 命令行 server，不带 GUI 启动完整应用服务，浏览器打开启动时打印的带 token URL 即可使用全部功能；支持 `--bind`、`--port`、`--token`、`--allow-host` 参数和 `KXEN_DATA_DIR` 数据目录覆盖，远程访问可经 tailscale 终结 TLS。
- 桌面应用内置浏览器访问：桌面窗口与浏览器是同一内嵌服务的两个平等客户端，经同一个 `/ws` 端点使用全部功能。
- 系统托盘：打开窗口、在浏览器中打开、复制访问链接、浏览器访问开关、默认打开方式(窗口或浏览器)、关闭时最小化到托盘、检查更新和退出。
- `[web]` 配置节(浏览器访问开关、监听地址、端口)和 `[tray]` 配置节(默认打开方式、关闭时最小化到托盘)。
- 发布矩阵覆盖六个平台：macOS(Apple Silicon、Intel)、Windows(x64、ARM64)、Linux(x86_64、ARM64)；每个平台同时发布 `kxen` CLI 包，稳定命名为 `kxen-<os>-<arch>.tar.gz`(Windows 为 `.zip`)。
- macOS 的 `kxen` CLI 与桌面 App、DMG 一样经 Developer ID 签名和 Apple 公证。
- 官网新增 Web 模式指南和代码签名说明页，首页提供六个平台的完整下载清单。

### Changed

- 桌面窗口与浏览器统一使用同一个内嵌 axum 服务的 `/ws` WebSocket 端点，webview 与浏览器使用相同的前端传输层。
- 前端适配纯浏览器环境：自动更新、系统通知、原生对话框和拖放在浏览器中自动降级，项目选择改为输入路径，附件经浏览器文件选择控件；token 经 URL 一次性投递，存入 sessionStorage 后从地址栏抹除。
- Windows 安装包(NSIS)和 Linux 安装包(AppImage、deb)进入正式发布矩阵；Windows 版本暂不做 Authenticode 签名，SmartScreen 提示时选择 More info -> Run anyway。
- CI Rust 门禁扩展为 macOS、Ubuntu、Windows 三平台矩阵(测试在 macOS 和 Ubuntu 运行)，并先构建前端产物再编译 Rust，保证 rust-embed 静态资源一致。

### Removed

- `tauri-plugin-websocket` 与独立的 tungstenite 监听端口，由单一内嵌 `/ws` 端点取代。

### Fixed

- Linux 上 Hooks 和 dev server 不再硬依赖 `/bin/zsh`，按 zsh -> bash -> sh 探测可用解释器。
- Linux 上进程组终止信号不再静默丢失：GNU kill 需要 `--` 分隔符才能正确解析负数 pgid，此前该平台上的组终止实际为空发并泄漏子进程。

## [0.0.1]

### Added

- 按 Workspace 隔离的 MCP、LSP 和 Hooks runtime registry。
- 持久化的 Session pending queue、retry、删除 tombstone、recovery bundle 和恢复导入。
- Goal completion contract、预算记账、Subagent、Dynamic Workflow journal 和 Agent Teams 持久化。
- 全局 Approval host，以及 Session 与全局审批的断线恢复和原子裁决。
- Provider catalog、custom endpoint、OAuth refresh、MRM 健康状态和 usage 完整性标记。
- Session JSONL 与 PendingQueue 的 `recovery.inspect`、`recovery.repair`、`recovery.clear` 契约，以及 Composer 存储恢复面板。
- Frontend 与 Rust coverage gate、100 个 RPC 三方精确对账门禁和 Stream topic ACL 门禁。
- Developer ID 签名、公证、DMG、updater archive、latest.json、SHA256SUMS 和 GitHub Release 产物验证工具。
- 13 个条目的应用内 OAuth 登录（Anthropic、OpenAI、xAI、Kimi For Coding、GitHub Copilot、Qwen Code 订阅、Google Gemini 订阅、Google Antigravity、MiniMax 双区域、OpenRouter、AWS Kiro、智谱 Coding Plan），含 code flow、device flow 与 refresh 自动续期。
- 9 个 Coding Plan / 网关 API 条目：智谱、百炼、阶跃拆中国/国际双区域，豆包、千帆、腾讯 Coding Plan，Vercel AI Gateway、Hugging Face、Ollama Cloud。
- 工具执行历史分组卡片（ToolGroupCard/ToolCard），diff 与文件树渲染统一接入 @pierre/diffs 与 @pierre/trees。

### Changed

- Session 激活、run slot、queue claim、Approval winner 和 Goal 写入改为原子状态转换。
- WebSocket stream sequence 改为连接级状态，断线后通过后端快照和 sys.resync 恢复。
- 文本生成、摘要、embedding、Provider native search 和 cloud audio transcription 统一经过 MRM，并在 Session model 元数据损坏时失败关闭。
- Web、Provider、OAuth 和 Remote MCP 统一使用不继承环境代理的 guarded connector；Browser 全流量固定经过进程内受控代理。
- UI 数据面统一区分 loading、empty、error、last-good stale 和 UNKNOWN，旧异步响应不能覆盖新状态。
- macOS 发布改为只从 main 手动触发，对稳定 SemVer tag 的目标 commit、可信校验脚本和发布产物逐层复核。
- 产品文档统一由 website package 维护，维护者契约保留在 README、CONTRIBUTING、SECURITY 和 .agents 中。

### Fixed

- Session run 终态、queue ack/release/retry、删除恢复和模型选择的持久化顺序及故障传播。
- Session 消息和 PendingQueue 在 PostCommit 耐久性不确定时保留精确快照、备份原始 JSONL 并 fail closed。
- Queue 续跑的 run slot 旧 token -> 新 token 原子换代，终态与下一次 run 之间不暴露可抢占空窗。
- Approval timeout、abort、断线、重复响应和 commit 阶段之间的竞态。
- Provider 凭证 probe、consent、refresh、import、delete 和多来源并发覆盖。
- Keychain 探测改为可超时、可终止并显式回收的 `/usr/bin/security` 子进程。
- Provider 连接实测、角色试派发、native search、可能按请求计费的 Web Search API 和 Voice 云转写在网络前持久化 usage attempt；Voice 只有显式 cloud fallback 才上传 Apple 录音，并限制云转写缓冲大小和时长。
- MCP 配置损坏、OAuth token scope、transport 畸形响应、tools/call isError 和进程生命周期的失败关闭。
- 文件 canonicalization、symlink escape、大小上限、目录移动、权限保持、trash 和 snapshot 边界。
- Schedule、Team、Workflow、Goal、usage 和 config store 的原子写入、损坏隔离与错误可见性。
- Knowledge consolidation 的 Provider 结果 UNKNOWN、cursor checkpoint、usage receipt 和用户显式确认链路。
- 内置 Command 清单只展示真实可执行入口，加入后端 `/compact`，移除会被当成普通消息发送的伪命令。
- Workspaces、Session、Composer、Model、Usage、Schedule、Goal、Task 和 Settings 面板的假空态、stale response 和静默失败。
- Release draft ownership、tag 可变性检查、签名验证、archive 路径穿越和远端 asset 字节一致性。

### Removed

- 已被 Schedule durable dispatch 替代的 cron_dispatch 模块。
- 与当前代码和产品文档重复的临时实现计划文档。

[Unreleased]: https://github.com/StringKe/kxen/compare/v0.1.2...HEAD
[0.1.2]: https://github.com/StringKe/kxen/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/StringKe/kxen/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/StringKe/kxen/compare/v0.0.1...v0.1.0
[0.0.1]: https://github.com/StringKe/kxen/releases/tag/v0.0.1
