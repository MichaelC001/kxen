# Changelog

本项目的显著变更记录在此文件中。

格式遵循 https://keepachangelog.com/zh-CN/1.1.0/ ，版本遵循 https://semver.org/lang/zh-CN/ 。

## [Unreleased]

## [0.1.4]

### Added

- 独立 Bots 产品块，提供 Bot Library、Bot Build、Routine、Runs、Recovery 和统一管理界面；Bot definition 支持草稿、immutable revision、生命周期、输入输出契约、最小 capability/resource grant、预算、Memory 和手动运行。
- DCP BotRun 使用 provider-neutral context、revision/permission snapshot、durable turn、tool journal、Approval、Input、Cancel、Artifact、usage、terminal 和 restart recovery，副作用结果不确定时进入 `UNKNOWN` 而不是自动重放。
- Bot-to-Bot Direct 和 2 至 6 Bot Group，提供 reciprocal peer allowlist、moderator、mentions、`@everyone`、Message、Delivery、CollaborationTask 和异步 recipient BotRun 闭环；每个 Bot 保留自己的 revision、权限与预算。
- Bot Routine 支持 cron、IANA timezone、isolated/continue-conversation context、follow-current/pinned revision、Run now、幂等 occurrence 和连续失败自动暂停。

### Changed

- 抽取通用 identity、durability、event store、operation journal、delivery、scheduler、artifact 和 recovery primitives，并增加可由 Session、Subagent、Team、Workflow 与 Bot 复用的 Agent runtime、capability、DCP 和 domain tool 边界。
- MCP tool 暴露改为 exact server grant，Bot Connector 必须显式绑定 Workspace；Bots RPC、Stream topic ACL、frontend contract、README 和官网产品文档同步到同一行为模型。

### Fixed

- 修复 Bot cancellation 被迟到 failure 覆盖、未 settle 副作用被普通终态隐藏、重复 tool call identity 冲突，以及 restart recovery 对 cancellation 和 `UNKNOWN` 状态的结算问题。
- 修复 inactive Bot 仍接受任务或 Routine 派发、Direct policy 非 reciprocal、Group member/Task admission 漂移、Routine terminal 重复结算和 Builder evidence 未与精确 draft hash 绑定的问题。
- 修复 Bot 管理界面用未发布 draft 解释正式 Run 输入、Routine 手动运行 contract race、RPC filter 命名漂移和结构化 peer message 校验缺失。

## [0.1.3]

### Added

- Composer 无触发符上下文主动推荐，默认以完整 draft、近期 Session 文本、已选附件、先前上下文、最近 involved 文件、Git status/diff、路径、mtime 和本地摘要生成 Local 候选。
- 可选 Embedding semantic rerank 和 LLM prompt suggest。两者默认关闭，使用 Workspace MRM、durable usage accounting、Goal budget、RPM、Circuit、timeout、取消和独立 Workspace cache；LLM 文件候选严格限制为本地 shortlist id。
- Settings 高级区域提供 Composer Local、Embedding、LLM 开关及 embedding provider 配置，模型路由增加 `suggestion` 角色并回退 `chat`。

### Changed

- Trigger popup 保持最高优先级；主动推荐仅在光标位于末尾、非 IME、非录音和非 Session run 状态显示。`ArrowUp`/`ArrowDown` 选择、`Tab` 接受、`Escape` 关闭当前 draft，`Enter` 始终保留发送语义。
- 文件候选只转换为 row chip，文本候选只插入 Composer，不会自动发送。未信任 Workspace 只使用路径和 mtime；索引尊重 `.gitignore`，排除敏感路径且不跟随 symlink。

## [0.1.2]

### Changed

- 依赖对齐当前可升级 latest：`reqwest` 0.13.4（`rustls` 自带 platform verifier，移除已删除的 `rustls-native-certs`）、`getrandom` 0.4、Cargo.lock 兼容范围内传递依赖刷新。
- 前端 `@solidjs/router` 升级到 1.0.0（0.16 API 稳定版 major 对齐），并更新 `@pierre/diffs`、`shiki`、`lucide-solid`。
- 官网 `astro` 7.2.0、`wrangler` 4.120.1；`tsconfig` 去掉已弃用的 `baseUrl`，改用相对 `paths`。
- website `typescript` 保持 6.0.3：Astro language tools / `astro check` 尚未支持 TypeScript 7 programmatic API，待上游就绪后再升。

## [0.1.1]

### Added

- Agent-native Kanban：提供持久化 event log、projection、RPC/topic、前端看板、卡片详情、每卡独立 worktree、列执行器、自主授权策略和网站文档。
- Dynamic Context Protocol agent 定义：支持内置列 agent 与用户自定义 `permission_profile`，显式约束工具集、模型、prompt 和工作目录。
- Agent turn、subagent、teammate 和后台 task 的迭代级持久化与恢复；重启后可重建 tool interaction，并把中断或完成事实 durable 回投父 Session。
- 后台 `exec` 进程 journal 与启动时孤儿回收，workflow 未提供 `run_id` 时自动生成不命中旧缓存的隔离标识。

### Changed

- 前端时间线按 run 聚合迭代消息为视觉回合，并新增 Kanban 导航、策略编辑和状态订阅。
- Rust 热路径减少重复 `String`、`Vec` 和 payload 拷贝：共享只读文本、tool catalog、配置和 manager 快照，复用缓冲并避免不必要的中间集合。
- 升级 Rust、Frontend、GitHub Actions 和 Nimbus 文档依赖，并保持三平台 CI、网站构建和 release 资产校验通过。

### Fixed

- Kanban event log 增加跨进程锁、内容锚、闭集校验和 snapshot 加固，修复认领、落地、授权、补发、收养及并发恢复中的重复或遗漏状态。
- 后台进程采用 persist-before-deliver 顺序，在公开 exit code 前先持久化终态；恢复时识别已完成进程并回收仍存活的孤儿进程。
- 修复敏感路径在 symlink 替换后的缓存绕过、dev server 非 ASCII 增量输出 panic、同尺寸原子替换导致的配置缓存陈旧，以及 workspace 目录替换复用旧 runtime。
- 修复 SSE escaped JSON 错误识别、LSP UTF-8 trim 边界、Markdown 注入面、Frontend 异步竞态和通知落盘复用问题。
- 修复 release validator 的版本盲区，强制 `kxen-core`、`kxen-cli`、`kxen-gui`、Cargo.lock 和 Tauri 配置与 release tag 一致。

## [0.1.0]

### Added

- kxen server 的 Docker 多架构镜像（linux/amd64、linux/arm64）随每次 release 发布到 `ghcr.io/stringke/kxen`，默认以 Web 模式启动，数据持久化在 `/data` 卷。
- 无头 Web 模式：新增 `kxen` 命令行 server，不带 GUI 启动完整应用服务，浏览器打开启动时打印的带 token URL 即可使用全部功能；支持 `--bind`、`--port`、`--token`、`--allow-host` 参数和 `KXEN_DATA_DIR` 数据目录覆盖，远程访问可经 tailscale 终结 TLS。
- 桌面应用内置浏览器访问：桌面窗口与浏览器是同一内嵌服务的两个平等客户端，经同一个 `/ws` 端点使用全部功能。
- 系统托盘：打开窗口、在浏览器中打开、复制访问链接、浏览器访问开关、默认打开方式(窗口或浏览器)、关闭时最小化到托盘、检查更新和退出。
- `[web]` 配置节(浏览器访问开关、监听地址、端口)和 `[tray]` 配置节(默认打开方式、关闭时最小化到托盘)。
- 发布矩阵覆盖六个平台：macOS(Apple Silicon、Intel)、Windows(x64、ARM64)、Linux(x86_64、ARM64)；每个平台同时发布 `kxen` CLI 包，稳定命名为 `kxen-<os>-<arch>.tar.gz`(Windows 为 `.zip`)。
- macOS 的 `kxen` CLI 与桌面 App、DMG 一样经 Developer ID 签名和 Apple 公证。
- 官网新增 Web 模式指南和代码签名说明页，首页提供六个平台的完整下载清单。

### Changed

- 仓库结构：Cargo workspace 上移至仓库根，产品逻辑独立为 `crates/kxen-core` 库 crate，无头 server 为 `crates/kxen-cli`（产物仍名 `kxen`），`src-tauri` 仅保留 Tauri 桌面壳。
- 桌面窗口与浏览器统一使用同一个内嵌 axum 服务的 `/ws` WebSocket 端点，webview 与浏览器使用相同的前端传输层。
- 前端适配纯浏览器环境：自动更新、系统通知、原生对话框和拖放在浏览器中自动降级，项目选择改为输入路径，附件经浏览器文件选择控件；token 经 URL 一次性投递，存入 sessionStorage 后从地址栏抹除。
- Windows 安装包(NSIS)和 Linux 安装包(AppImage、deb)进入正式发布矩阵；Windows 版本暂不做 Authenticode 签名，SmartScreen 提示时选择 More info -> Run anyway。
- CI Rust 门禁扩展为 macOS、Ubuntu、Windows 三平台矩阵(测试在 macOS 和 Ubuntu 运行)，并先构建前端产物再编译 Rust，保证 rust-embed 静态资源一致。

### Removed

- `tauri-plugin-websocket` 与独立的 tungstenite 监听端口，由单一内嵌 `/ws` 端点取代。

### Fixed

- 官网生产依赖审计：`postcss` 链的 `nanoid` 高危告警（size 为 0 时死循环），通过 pnpm override 锁定 `>=3.3.17`。
- Linux ARM64 发布构建：补齐 `xdg-utils`（deb/AppImage bundler 调用 `xdg-open`），该 runner 镜像不自带。
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

[Unreleased]: https://github.com/StringKe/kxen/compare/v0.1.4...HEAD
[0.1.4]: https://github.com/StringKe/kxen/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/StringKe/kxen/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/StringKe/kxen/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/StringKe/kxen/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/StringKe/kxen/compare/v0.0.1...v0.1.0
[0.0.1]: https://github.com/StringKe/kxen/releases/tag/v0.0.1
