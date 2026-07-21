# kxen Rust 重构设计（0 -> 1）

版本: 1.0
日期: 2026-07-21
状态: 待用户确认（确认后才写代码）

## 0. 前提与教训（为什么这样设计）

上一方案（OpenCode fork + daemon/浏览器）暴露的结构性问题，本设计逐条规避：

| 教训 | 对策 |
| --- | --- |
| daemon + HTTP server 带来端口/权限/file API 故障面 | 无 daemon、无端口、无 HTTP server：Tauri 单进程，Rust 核心经 IPC 直调 |
| 371MB 内存 / 1036ms 首帧（OpenCode 实测） | Rust + WKWebView：目标内存 < 80MB 常态、首绘 < 500ms、安装包 < 20MB |
| 2475 个 npm 依赖 + 13 个 patch 的脆弱链 | cargo 依赖控制在 30 个以内，全部稳定版 |
| 运行时语言（TS）的工具调用开销与内存抖动 | 零 GC；hot path 无分配纪律（见第 6 节） |
| 上游 4798 open issues 的毛边（pty flake、locale、embedded） | 自研小内核，只保留被证明有效的机制 |

kxen 资产的复用方式：**逻辑移植，不翻译代码**。safety 规则集（F1-F5）、goal 状态机、workflow runtime 语义、mrm 调度算法、凭证格式（四家官方 CLI 存储位置已实测）、docs/ 全部调研作为需求基线。

## 1. 形态

- **Tauri 2.x 单 GUI app**：Rust 核心 + WKWebView GUI（macOS 系统 WebKit，mermaid/任何现代 Web 能力直接用）。**仅 macOS Apple Silicon（M 系列，aarch64-apple-darwin）**——不做 x86_64、不做 universal binary，专精单平台吃满其优势（包更小、构建更快、可按 M 系列特性优化）。
- **无 CLI**：不提供任何命令行入口。doctor 为 GUI 内的状态页（凭证/环境自检）；stop 即 app 退出；upgrade 走 tauri-plugin-updater（GitHub Releases 源，app 内提示更新）。无 daemon、无端口、无 serve/web。
- 前端：轻量静态页（vanilla TS + marked + mermaid），不打包框架运行时；Tauri 内嵌资产。

## 2. Cargo workspace 布局

```
crates/
  kxen-core      域模型与状态：session/message/part、goal 状态机、config（TOML）、事件总线
  kxen-llm       provider 抽象与调用：Rig 适配 + 订阅 Bearer 注入/刷新 + mrm 调度（并发/降级/状态）
  kxen-auth      四订阅凭证：Keychain（keyring crate）+ codex/grok/kimi 文件读取、新鲜度比较、refresh
  kxen-tools     工具层：exec（多 shell）/read/write/edit/glob/grep + safety 硬拦截（F1-F5）
  kxen-agent     agent loop、tool 调度、subagent（角色化）、workflow runtime（rquickjs）
  kxen-app       Tauri 壳：commands/events、窗口、菜单、updater
```

依赖方向单向：app -> agent -> tools/llm/auth -> core。core 不依赖任何上层。

## 3. 外部依赖选型（全部已核实，2026-07-21）

| 用途 | 选型 | 版本 | 备注 |
| --- | --- | --- | --- |
| app 框架 | tauri | 2.x | WKWebView；签名公证走 tauri bundler |
| LLM provider | rig-core | 最新 | 20+ provider 统一 API、tools、streaming、MCP；订阅 OAuth 自写薄层注入 |
| HTTP | reqwest | 0.12 | rustls（不带 OpenSSL） |
| macOS Keychain | keyring | 4.1.5 | Claude 凭证读取 |
| PTY | portable-pty | 0.9.0 | wezterm 出品，exec 工具用 |
| 脚本引擎 | rquickjs | 最新 | workflow 编排（模型写 JS：agent()/pipeline()/constraints()/phase()） |
| 序列化 | serde/serde_json | 1 | |
| TOML | toml | | 配置 |
| 异步 | tokio | 1 | rt-multi-thread |
| 正则 | regex/regexset | 1 | safety 规则，OnceLock 预编译 |
| 文件监听 | notify | | .agents/ 变更（后期） |
| 日志 | tracing | | |

明确不引入：deno_core（重）、boa（性能弱于 QuickJS）、任何 JS 运行时打包、openssl。

## 4. 核心机制设计

### 4.1 订阅接入（kxen-auth）

- Claude：Keychain `Claude Code-credentials`（keyring）或 `~/.claude/.credentials.json`；refresh 走 `https://console.anthropic.com/v1/oauth/token`（client_id 已知）；调用注入 `Authorization: Bearer` + `anthropic-beta: oauth-2025-04-20`。
- Codex：`~/.codex/auth.json`（tokens.{access_token, refresh_token, account_id}）；refresh 走 `https://auth.openai.com/oauth/token`（client_id 已知），调用带 `ChatGPT-Account-Id` 头，端点 `https://chatgpt.com/backend-api/codex/responses`。
- Grok：`~/.grok/auth.json`（issuer map，取 expires 最新一条）；xai API Bearer。
- Kimi：`~/.kimi-code/credentials/kimi-code.json`；`https://api.kimi.com/coding/v1` Bearer。
- 每次调用前比新鲜度（expires 大者优先），官方 CLI 轮换自动跟进（已验证模式）。
- Rig 的 reqwest client 注入层统一加 Bearer 与刷新钩子；Anthropic 订阅走 Rig anthropic provider + 自定义 header 中间件。

### 4.2 mrm（全局模型资源管理）

- `Roles`（config.toml [roles]）：thinking/planning/execution/review/research -> provider/model。
- 调度：tokio Semaphore per provider（并发上限）+ 滑动窗 RPM + 降级链（角色 fallback）。
- 状态：mrm.describe() 注入 planning 模型上下文（低成本静态字符串）。
- 所有 LLM 调用与 subagent 派发经过同一 acquire/release（Rust RAII guard 自然释放）。

### 4.3 safety（执行层硬拦截）

- kxen-safety 的 F1-F5 规则集原样移植：命令分段（| && || ;）、危险命令识别、保护路径清单（系统区/用户目录/凭证目录/.git + macOS 豁免 /private/var/folders、/private/tmp、/dev/null）。
- 实现：`RegexSet`（OnceLock 预编译）+ `&str` 切片扫描，**热路径零分配**；命中返回结构化错误（ruleId + reason + suggestion）。
- 三层拦截：exec 命令文本、write/edit 路径守卫（resolve 后比对）、防绕过（bash -c/eval 递归）。
- 不做内容级风控（逆向/破解/外挂不拦）。

### 4.4 goal（生命周期）

- 状态机（kxen-goal 已验证语义）：draft/queued/active/paused/complete/blocked/budget_limited/canceled + 完成契约必填 + 阻塞三次规则（同一原因 3 次才 blocked，terminal 例外）+ 预算三维（tokens/turns/wallClock）。
- 持久化：`~/Library/Application Support/kxen/goals/<id>.json`（macOS 规范目录）。
- 注入：active goal 渲染进系统上下文（占位形态避开首基线问题——Tauri 无此问题，直接条件注入）。
- /write-goal：GUI 内引导流程（AskUserQuestion 形态由前端承载，复用已验证的五要素契约模板）。

### 4.5 workflow（动态编排）

- rquickjs 沙箱执行模型写的 JS：`agent(prompt, {role?})` / `pipeline(items, fn, {concurrency})` / `constraints()` / `phase(name)` / `args`。
- agent() 经 tokio 桥到 agent loop（QuickJS <-> tokio：spawn_blocking + channel）。
- 护栏：单次 run agent 调用上限（默认 200）、每调用超时、调用缓存（resume 回放，已验证语义）。
- 中间结果留在 JS 侧，主上下文只收 return 值。

### 4.6 exec 多 shell

- `exec(type: zsh|bash|fish, path, command, timeout?)`：type 必填；macOS 无 cmd/powershell 需求（仅 mac）。
- 方言校验器（fish 无 export、zsh 数组 1-index 等）+ safety 同层拦截。
- portable-pty 承载交互式命令；输出 30KB 截断 + 大输出落临时文件（已验证模式）。

### 4.7 subagent

- opencode 验证过的形态：角色 agent（kxen-thinking 等）预定义（model 绑定 + 权限预设 + prompt），经 mrm 路由。
- task 工具派发；深度限制防递归。

### 4.8 .agents/ OKF

- 移植 kxen-agents 解析（gray-matter 等价：Rust 手写 frontmatter 解析，~100 行）。
- rules alwaysApply 注入 + 渐进披露索引；启动无 .agents 则跳过（避开首基线坑的设计已验证）。

## 5. GUI（前端）

- Tauri 内嵌静态页：会话列表/会话视图/流式渲染（marked + mermaid）/角色与模型选择器/goal 面板/doctor 页。
- Rust -> 前端：Tauri events（`llm://delta`、`tool://call`、`goal://update`）；前端 -> Rust：commands（send_message / dispatch_task / goal_action / doctor）。
- 目录选择：纯 Rust 实现（read_dir 经 command 返回，无上次的 HTTP 层）。

## 6. 性能与安全纪律（无 Clone 原则）

- **hot path 零分配**：safety 扫描、SSE 解析、diff 渲染只用 `&str`/`&[u8]` 切片；共享字符串用 `Arc<str>`（clone 仅计数）；消息部件 `Box<[T]>`。
- **Regex/RegexSet 全部 OnceLock 预编译**；HTTP client 全局单例（连接池）。
- **事件流零拷贝**：tokio broadcast channel；大输出只传路径不传内容。
- **编译优化**：`opt-level = 3`、`lto = "thin"`、`codegen-units = 1`、`strip = true`、`panic = "abort"`（发布 profile）。
- 目标：安装包 < 20MB、常态内存 < 80MB、首绘 < 500ms、agent 首 token < 2s（本地条件）。
- 安全：Tauri capabilities 最小授权（前端仅能调白名单 command）；凭证只读 Keychain/0600 文件；exec 经 safety 硬拦截；无远程更新自执行（upgrade 仅拉源码+重建提示）。

## 7. 里程碑（0 -> 1 渐进，每个可验证）

| 里程碑 | 内容 | 验证 |
| --- | --- | --- |
| M0 | workspace + Tauri 空窗 + kxen-auth 四订阅读取 + doctor 状态页 | 状态页显示四家凭证状态 |
| M1 | kxen-llm 单 provider（xai Bearer）+ 流式到 GUI | GUI 发一条消息收到流式回复 |
| M2 | agent loop + exec/read/write/edit + safety | 模型调工具改文件 + rm -rf / 被拦 |
| M3 | 四订阅全接入 + mrm 角色路由 + subagent | 四家各一次真实调用；角色 agent 派发 |
| M4 | goal 状态机 + 注入 + /write-goal 流程 + workflow（rquickjs） | goal 全生命周期；workflow 编排真实跑通 |
| M5 | .agents/OKF + exec 方言校验 + 打磨（更新器、签名公证） | OKF 注入可见；release 包 < 20MB |

## 8. 明确不做

- 不做 Windows/Linux/Intel Mac（仅 macOS Apple Silicon）
- 不做 CLI、不做 TUI、不做 daemon、不做 HTTP API、不做插件市场
- 不做内容级提示词风控
- 不做移动端（Tauri iOS 留未来选项）

## 9. 开放问题

1. Rig 对 codex 订阅端点（chatgpt.com/backend-api）的适配度——若不合，codex 单独自写（~200 行），其余走 Rig。
2. rquickjs 的 tokio 桥接形态（agent() 同步转异步）——M4 首个技术验证点。
3. 更新渠道：tauri-plugin-updater + GitHub Releases（签名 dmg，app 内提示），或仅手动下载。

## 10. 参考实现（2026-07-21 实查，按借鉴价值排序）

| 项目 | 关键数据 | 借鉴点 |
| --- | --- | --- |
| 1jehuang/jcode（9.6k stars，Rust 93.5%） | 首帧 14ms（OpenCode 的 1/74）；117MB / 10 sessions（OpenCode 的 1/27） | **性能基准**：kxen 目标数字的直接参照；OAUTH.md + auth 模块是四订阅 OAuth 的现成 Rust 实现参考；单 crate 多模块的轻结构 |
| KonghaYao/peri（Apache 2.0） | 13MB 二进制、~50MB RAM、95-99% cache 命中 | **形态最接近 kxen**：Rust + Goal + Dynamic Workflow + Claude Code 兼容。重点读它的 Goal/Workflow 形态和缓存设计（frozen system prompt + boundary marker -> 高 cache 命中） |
| Kuberwastaken/claurst（10k stars，GPL-3.0） | clean-room 重实现 Claude Code | **spec 驱动方法**：spec/ 行为规格先行。kxen 的 docs/ 调研即 spec 基线（仅方法借鉴，license 不兼容不取代码） |
| Dicklesworthstone/pi_agent_rust（1.4k stars） | 21MB、<50MB idle、零 unsafe | 自写 SSE parser（~150 行，少一个依赖）；**hashline_anchored edit**（LINE#HASH 锚点编辑，比 string match 稳，edit 工具借鉴） |
| openai/codex（codex-rs） | OpenAI 官方 Rust 核心 | workspace 领域划分参考（agent/protocol/tools/secrets/shell-command），不学它的 crate 数量（50+ 过重） |
| junhoyeo/uira | macOS Seatbelt 原生沙箱 | score-based goal verification 思路（goal 完成评分环）；Seatbelt 备查（kxen 不做沙箱，safety 拦截代替） |
| xai-org/grok-build（21k stars） | xAI 官方 harness，Rust | grok-build 订阅的 xAI OAuth 接入参考（用户主力模型） |

补充决策（本轮调研后修订）：

- **SSE 解析自写**（pi_agent_rust 模式，~150 行）——reqwest stream + 手写 parser，不引 eventsource 依赖。
- **缓存友好上下文**（peri 模式）：系统上下文分 frozen 段（capabilities/规则，整会话不变）与 dynamic 段（goal/.agents/mrm 状态，boundary marker 分隔），保 provider prompt cache 命中率。
- **edit 工具用 hashline 锚点**（pi_agent_rust 模式）：read 输出行号 + 内容 hash，edit 按锚点定位，消除 string match 的歧义与陈旧。
- **jcode/peri 实现参考仓库**（仅参考，不移植代码）：实现阶段 clone 到 SelfCode 下对照阅读。
