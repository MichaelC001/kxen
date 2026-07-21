# kxen Rust 重构设计（0 -> 1）

版本: 2.0（优点收纳收敛版）
日期: 2026-07-21
状态: 待用户确认（确认后才写代码）

**定位**：kxen 综合当前所有开源 agent-cli 的优点——Claude Code 的 Dynamic Workflow 与 sub-agents、Kimi Code 的 Goal 生命周期、grok-build 的命令调度速度、jcode 的性能、peri 的缓存命中、OpenCode 的 provider 广度与 system-context、pi_agent_rust 的轻量工程——在 macOS Apple Silicon 上以 Tauri 单 GUI app 交付。

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

### 4.1 provider 与订阅接入（kxen-llm + kxen-auth）

**provider 层是通用的，不特殊化任何一家**：Rig 的 20+ provider 与 openai-compatible 端点全部可用；用户当前恰好持有四订阅，仅意味着订阅导入层当前有四条探测规则。

**订阅导入（kxen-auth）= 通用的「官方 CLI 凭证探测」机制**：每个订阅一条探测规则（读官方 CLI 的凭证存储 -> 比新鲜度 -> 导入），新增订阅只加一条规则，不改架构。当前四条：

- Claude：Keychain `Claude Code-credentials`（keyring）或 `~/.claude/.credentials.json`；refresh 走 `https://console.anthropic.com/v1/oauth/token`（client_id 已知）；调用注入 `Authorization: Bearer` + `anthropic-beta: oauth-2025-04-20`。
- Codex：`~/.codex/auth.json`（tokens.{access_token, refresh_token, account_id}）；refresh 走 `https://auth.openai.com/oauth/token`（client_id 已知），调用带 `ChatGPT-Account-Id` 头，端点 `https://chatgpt.com/backend-api/codex/responses`。
- Grok：`~/.grok/auth.json`（issuer map，取 expires 最新一条）；xai API Bearer。
- Kimi：`~/.kimi-code/credentials/kimi-code.json`；`https://api.kimi.com/coding/v1` Bearer。

每次调用前比新鲜度（expires 大者优先），官方 CLI 轮换自动跟进（已验证模式）。Rig 的 reqwest client 注入层统一加 Bearer 与刷新钩子；Anthropic 订阅走 Rig anthropic provider + 自定义 header 中间件。

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

### 4.6 命令调度体系（grok-build 源码实证重组）

对 grok-build（xai-org/grok-build）源码的完整分析（`crates/codegen/xai-grok-tools/src/implementations/grok_build/bash/mod.rs`、`computer/local/`、scheduler）得出它「特别快」且模型不写 sleep/poll 管道的五个机制，kxen 全部采纳：

1. **auto_bg 自动后台化**：前台命令设阻塞预算（默认 15s），超时未退出**自动转后台**返回 task_id，agent 永不被长命令卡住。background 模式 timeout: 0 = 不限时。
2. **完成通知代替 sleep/poll**：任务完成经事件主动通知（kxen 为 Tauri event 到 agent 与 GUI），工具描述铁律写明 "do not poll or sleep-wait for it"——模型从机制上不需要写 `sleep 5 && curl x` 这类管道等待。
3. **任务工具三件套**：`exec(background) -> task_id`、`task_output(id)`、`kill_task(id)`——后台任务全生命周期，与 auto_bg 配套必须同时存在（grok-build 用 requires_expr 声明此依赖）。
4. **静态快照 shell（主路径）**：启动时一次性捕获用户 login shell 的函数/alias 快照，每条命令在 fresh shell replay 快照 + cd workdir——无跨命令状态污染、subagent 并发天然安全；需要 cd/env 保持的少数场景才用持久会话。
5. **命令遮蔽**：`grep -> ugrep`、`find -> bfs`（二进制存在才启用，shell function + marker 门控 restore，不覆盖用户自定义函数）——模型按习惯写慢命令，实际执行快实现。macOS 上 ugrep/bfs 由 brew 检测，缺失则静默关闭。

流式与输出纪律（同源实证）：100ms tick 节流 + 16KB/tick 增量上限 + 总量截断 + 大输出落会话文件只回路径。

### 4.7 dev server 管理（长运行进程）

场景：agent 写完代码起 dev server 验证（vite / cargo watch / next dev / bun --hot）。这类进程长期运行、有就绪信号、有端口、要看日志、要重启——4.6 的 auto_bg 只解决「不卡住」，本节解决「管得好」。

**dev_server 工具**（长运行专用入口）：

- 参数：`{command, workdir, ready?: {pattern?: string, port?: number, timeout_ms?: number}}`
- 行为：后台启动 -> **阻塞等待就绪**（输出匹配 ready.pattern 或端口可达）-> 返回 `{task_id, url?, pid}`。agent 立即拿到可用地址，永远不写 sleep 等就绪。
- 就绪失败（超时/进程提前退出）-> 错误 + 日志尾部，agent 直接进排查。
- 就绪 pattern 默认集：`listening|ready|started|watching|serving|compiled`；端口从输出解析（`:(\d{4,5})`）或显式指定。

**任务族工具补全**：

- `restart_task(id)`：同配置重启（大改动 dev server 不热载时），保持 task_id 不变。
- `list_tasks()`：全部后台任务状态表（id / 命令 / 状态 / uptime / 端口 / 输出尾部）——agent 与 GUI 共用同一份。
- `monitor`（采纳 grok-build 模式）：对长运行脚本做行级事件流（每行一条事件通知，`persistent: true` 为会话级）；带速率限制；描述里提醒管道过滤用 `grep --line-buffered`（plain grep 会缓冲数分钟）。
- 健康检查：dev_server 注册的任务每 30s 探测端口，失连即发事件（server 崩溃 agent 立刻知道，而不是下次调用才发现）。

**GUI 后台任务页**：dev server 列表（状态灯 / 命令 / 端口 / uptime / 日志尾部），操作：停止 / 重启 / 查看完整日志；任务完成与崩溃作为系统消息出现在会话流里。

### 4.8 exec 工具

- `exec(type: zsh|bash|fish, path, command, timeout?, background?)`：type 必填；macOS 无 cmd/powershell 需求（仅 mac）。
- 方言校验器（fish 无 export、zsh 数组 1-index 等）+ safety 同层拦截。
- 执行走 4.6 的静态快照 shell + auto_bg；输出按 4.6 流式纪律。
- 工具描述模板：单条良构命令优先于串联长命令；长任务用 background；禁止 sleep/poll 等待（完成会通知）。

### 4.9 subagent

- opencode 验证过的形态：角色 agent（kxen-thinking 等）预定义（model 绑定 + 权限预设 + prompt），经 mrm 路由。
- task 工具派发；深度限制防递归。

### 4.10 .agents/ OKF

- 移植 kxen-agents 解析（gray-matter 等价：Rust 手写 frontmatter 解析，~100 行）。
- rules alwaysApply 注入 + 渐进披露索引；启动无 .agents 则跳过（避开首基线坑的设计已验证）。

### 4.11 loop 检测与 goal 验证

- **4 层循环检测**（rust-code 模式）：exact（同调用重复）/ semantic（近似意图重复）/ output stagnation（输出不再变化）/ frequency churn（高频无效调用）。命中即中断并回写原因（"检测到循环：{layer}，已停止。建议换路"），防 agent 空转烧 token。
- **score-based goal 验证**（uira 模式）：goal 完成判定不走模型一句话，逐条 proof 打分（每条 completionCriteria 独立验证命令/搜索/状态），全部通过才允许 complete。

### 4.12 会话管理

- JSONL 持久化（每消息一行，append-only）+ branch/fork/resume（pi_agent_rust 模式）。
- compaction：超阈值时 LLM 摘要 + 规则裁剪（工具大输出换路径引用），压缩后 frozen 段与关键约束重注入（context 工程 C7 决策）。

### 4.13 hooks

- 事件钩子（全面管控 + 可选开启，analysis/08）：pre_tool_use / post_tool_use / notification / stop / session_start。
- 配置在 config.toml [hooks]；全部默认关闭，开启即生效；hook 是本地命令（经 safety 同一道拦截）。

### 4.14 渐进披露（Tool Search 模式）

- 常驻工具 ~12：exec / read / write / edit(hashline) / glob / grep / task / todo / goal / workflow / webfetch / websearch。
- 其余（LSP ops、MCP 工具、dev_server、scheduler 等）经 `tool_search` 按需发现并临时挂载——上下文只放当前需要的工具卡，保 prompt cache 命中（peri 实证 95-99%）。

### 4.15 worktree 隔离

- 批量迁移/并行修改类任务（workflow pipeline 的常规形态）：project copy（git worktree）隔离执行，完成后 diff 回主树——并行 subagent 之间零冲突。

### 4.16 读写删工具（各家对比后的重组）

**各家现状**（2026-07-21 实查）：

| 工具 | Read 形态 | Edit 形态 | 问题 |
| --- | --- | --- | --- |
| Claude Code / OpenCode | 行号 + 截断 | old_string 精确匹配 + 强制先 Read | 模型凭记忆 edit 被拒，浪费一整轮（用户痛点） |
| grok-build hashline | LINE#HASH 锚点（三 scheme：ContentOnly / ChunkFingerprint / CheckpointChain） | 锚点定位 + `find_shifted` 有界窗口恢复 | 当前最强实现 |
| pi_agent_rust | LINE#HASH | 锚点编辑 | 同源思路（grok-build 更成熟） |
| aider / Codex | repo map | whole/diff/udiff/apply_patch 按模型选格式 | patch 解析容错重 |

**kxen 采纳（hashline 优先 + 三保险）**：

1. **read**：输出 `LINE#HASH` 锚点，scheme 取 grok-build 推荐的 ChunkFingerprint（行内容 hash + 固定 chunk 指纹——上方编辑不影响下方锚点）；截断 2000 行 / 长行 2000 字符。
2. **edit 双模式**：锚点模式 `edits: [{anchor, new_text}]`（优先，无歧义）+ 兼容模式 `old_string/new_string + expected_replacements`（模型习惯兼容）。
3. **免 read-before-edit**：会话内文件状态跟踪（path -> mtime + size + 锚点快照）。会话内读过且未外部变更 -> 直接 edit；有外部变更 -> 仅提示并重读相关段（不强制完整 Read 再走一轮）。
4. **失败自愈**：锚点失配时自动 `find_shifted`（有界窗口找回），返回实际行内容与新锚点——模型下一轮直接改对，不需要补 Read 轮。

**delete = trash（macOS 专精，grok-build 遮蔽模式）**：

- exec 静态快照 shell 注入 `rm` -> `trash` 遮蔽（marker 门控 restore，不覆盖用户自定义）：模型写 `rm` 实际进回收站，**一切删除可恢复**。
- safety 协同：trash 删除按「可恢复」降档（approval 而非 forbidden）；`.git` / 系统路径的 trash 仍 forbidden（进回收站也不允许）。
- 实现：`/usr/bin/trash`（macOS 14+ 自带）优先；`trash` crate v5.2.6 备选；write/edit 的删除类操作同走 trash。

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
| M2 | agent loop + 命令调度体系（静态快照 shell + auto_bg + 任务三件套 + dev_server 管理）+ exec/read/write/edit + safety | 模型调工具改文件 + rm -rf / 被拦 + 长命令自动后台化（无 sleep/poll）+ dev server 起停/就绪/重启可演示 |
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

## 10. 优点收纳矩阵（全部调研的最终收敛）

逐维度对比开源 agent-cli 的最强点与 kxen 的采纳落点（调研依据：docs/research、docs/analysis、grok-build 源码分析、jcode/peri/claurst/pi_agent_rust/uira/aether 实查，2026-07-21）。

| 维度 | 最佳来源 | kxen 采纳 |
| --- | --- | --- |
| 形态 | Tauri（自定） | 纯 GUI app，仅 macOS Apple Silicon，无 CLI/daemon/端口 |
| 性能 | jcode | 目标：安装包 < 20MB、常态内存 < 80MB、首绘 < 500ms（jcode 实测 14ms 首帧 / 117MB 为基准） |
| 命令调度 | grok-build（源码实证） | auto_bg 15s 自动后台化、完成通知代替 sleep/poll、任务三件套、静态快照 shell、命令遮蔽（find->bfs/grep->ugrep） |
| dev server | grok-build + 自定 | dev_server 就绪等待（pattern/端口）、restart_task、list_tasks、30s 健康检查、GUI 任务页 |
| 编排 | Claude Code | Dynamic Workflow：模型写 JS（rquickjs 执行），agent()/pipeline()/constraints()/phase()，中间结果在脚本变量，缓存恢复，200 调用护栏 |
| 目标管理 | Kimi Code | goal 生命周期 + 预算三维 + 阻塞三次规则 + write-goal 契约流程（AskUserQuestion 驱动）+ goal 状态注入 |
| 子代理 | Claude Code + OpenCode | 角色化 subagent：thinking/planning/execution/review/research，各绑 model/permission/prompt 预设，task 派发 |
| 模型调度 | 自定（analysis/03） | mrm：per-provider 并发 semaphore + RPM 滑窗 + 角色降级链 + 状态注入规划模型；一切调用经 acquire/release |
| provider | OpenCode + jcode | 全通用（Rig 20+ + openai-compatible），订阅导入 = 通用探测规则机制（当前四条，新增加规则） |
| context 工程 | OpenCode + peri + DCP | frozen 段（能力/规则，整会话不变）+ boundary marker + dynamic 段（goal/.agents/mrm）保 prompt cache 命中；mid-conversation system message 模式；中间结果不进主上下文 |
| 渐进披露 | peri | 核心工具常驻（~12），其余按需发现（Tool Search 模式），省 token 保 cache |
| 编辑工具 | grok-build hashline + pi_agent_rust | ChunkFingerprint 锚点 read + 锚点/兼容双模式 edit + 会话内新鲜度跟踪免强制 read-before-edit + find_shifted 失败自愈 |
| 删除语义 | grok-build 遮蔽 + macOS | exec 遮蔽 `rm` -> `trash`（/usr/bin/trash 自带），删除可恢复；safety 对 trash 降档 approval，.git/系统路径仍 forbidden |
| 命令策略 | Codex execpolicy + 自定 | safety F1-F5 规则族硬拦截（毁系统/毁目录/删 .git），结构化错误返回；不做内容级风控 |
| loop 检测 | rust-code | 4 层循环检测（exact / semantic / output stagnation / frequency churn），防 agent 空转 |
| goal 验证 | uira | score-based verification：完成判定打分环（proof 逐条过） |
| 会话 | pi_agent_rust + OpenCode | JSONL 持久化 + branch/fork/resume；会话投影与 compaction（LLM 摘要 + 规则裁剪） |
| 提示词组装 | aether + analysis/07 | 分层注入：frozen 能力卡 + dynamic 状态；只陈述机制边界，无内容风控 |
| hooks | Claude Code + OpenCode | 事件钩子（全面管控 + 可选开启）：pre/post tool、notification、stop |
| LSP/MCP | OpenCode + OMP | 原生 auto-detect + 渐进披露（MCP 工具列入 Tool Search 而非全量进上下文） |
| worktree | OpenCode | project copy 隔离（批量迁移类任务的并行安全） |
| .agents/ OKF | 自定（design/09） | rules 注入型 + references 按需 + index.md 渐进披露 + 多层目录就近 |
| scheduler | grok-build | cron 式定时任务（M5 后增强项） |
| spec 驱动 | claurst | docs/ 全部调研即行为 spec（本方法论，已在执行） |
| 会话分享/遥测/云 | 多家 | 一律不做（个人自用，零遥测零上传） |

**kxen 的独特交集**（任何单一开源工具不具备的组合）：

1. Dynamic Workflow + Goal 生命周期 + mrm 全局调度 + dev server 管理 四者同体
2. jcode 级性能与 Claude/Kimi 级编排在同一个 Tauri app 里
3. 订阅通用探测 + 全 provider 广度，不特殊化任何一家
4. safety 执行层硬拦截（无内容级风控）+ Apple Silicon 专精

## 11. 参考实现（源码对照阅读库，SelfCode 下）

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
