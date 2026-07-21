# kxen 设计（Rust，0 -> 1）

版本: 3.0（一次成稿）
日期: 2026-07-21
状态: 待用户确认（确认后才写代码）

## 1. 定位与目标

kxen 是 macOS Apple Silicon 专精的 Coding Agent Harness，综合当前所有开源 agent-cli 的优点：Claude Code 的 Dynamic Workflow 与 sub-agents、Kimi Code 的 Goal 生命周期、grok-build 的命令调度速度、jcode 的性能、peri 的缓存命中、OpenCode 的 provider 广度与 system-context、pi_agent_rust 的轻量工程。

形态：Tauri 2.x 纯 GUI 单 app（Rust 核心 + WKWebView）。仅 aarch64-apple-darwin。无 CLI、无 TUI、无 daemon、无端口、无 HTTP server。

目标指标（以 jcode 实测为基准）：

- 安装包 < 20MB
- 常态内存 < 80MB
- 首绘 < 500ms
- agent 首 token < 2s（本地条件）

独特交集（任何单一开源工具不具备）：

1. Dynamic Workflow + Goal 生命周期 + 全局模型调度 + dev server 管理四者同体
2. jcode 级性能与 Claude/Kimi 级编排在同一个 app 里
3. 订阅通用探测 + 全 provider 广度，不特殊化任何一家
4. safety 执行层硬拦截（无内容级风控）+ Apple Silicon 专精

上一方案（OpenCode fork + daemon/浏览器）的教训对应：无 daemon 消掉端口/权限/file API 故障面；Rust 消掉 371MB 内存与 JS 运行时开销；自研小内核消掉上游 4798 open issues 的毛边与 2475 个 npm 依赖的脆弱链。

## 2. 架构

### 2.1 单 crate 模块布局（根即 crate 根，单向依赖）

单二进制应用没有外部消费方，不分库 crate；`app/` 一个 crate，库目标按域分文件夹：

```
仓库根即 crate 根（无 workspace 包装）：
  Cargo.toml        唯一定义（[package] + 依赖 + profile）
  src/lib.rs        库目标（pub mod 五个域）
  src/main.rs       Tauri 壳装配（commands/events/窗口/菜单/updater，无 src-tauri 层）
  src/agent/        agent loop / tool 调度 / subagent / workflow runtime
    -> src/tools/   exec / 读写删 / safety / hooks / worktree
    -> src/llm/     provider 调用 / 订阅注入与刷新 / mrm 调度
    -> src/auth/    订阅凭证探测 / 新鲜度 / refresh
      -> src/core/  域模型 / session / goal / config / 事件总线
  tauri.conf.json / build.rs / icons/ / gen/ / examples/  全部平铺根目录
  ui/               前端（vite-plus + SolidJS，独立 JS 项目）
```

### 2.2 外部依赖（2026-07-21 crates.io 核实 + 对比分析；总量控制 30 内）

| 用途 | 选型 | 核实数据 | 结论依据 |
| --- | --- | --- | --- |
| app 框架 | tauri 2.11.5 + tauri-plugin-updater 2.10.1 | 22.5M / 6.6M 下载，2026-07 活跃 | WKWebView macOS；updater 官方 |
| HTTP | reqwest 0.13.4 | 590M 下载，2026-05 | rustls（零 OpenSSL） |
| macOS Keychain | keyring 4.1.5 | 17.3M 下载，2026-07-14 | Security.framework 跨平台抽象（vs 直接用 security-framework crate：多一层但 API 更稳） |
| PTY | portable-pty 0.9.0 | 9.1M 下载，2025-02（稳定低更新） | wezterm 生产实证 |
| 脚本引擎 | rquickjs 0.12.1 | 2.8M 下载，2026-07-06 | 见下方对比分析 |
| 异步 | tokio 1.53.1 | 814M 下载，2026-07-20 | rt-multi-thread |
| 正则 | regex 1.13.1（内含 RegexSet，无独立 regex-set crate） | 987M 下载，2026-07-15 | OnceLock 预编译 |
| 文件监听 | notify 8.2.0 | 128M 下载，2026-05 | .agents/ 变更（后期） |
| 日志 | tracing 0.1.44 | 718M 下载 | |
| 序列化 | serde / serde_json / toml | 事实标准 | |
| SSE 解析 | 自写 ~150 行 | pi_agent_rust 模式 | 少一个依赖 |
| 回收站 | trash 5.2.6（备选） | 2.3M 下载，2026-05 | /usr/bin/trash（macOS 14+ 自带）优先，crate 备选 |

**脚本引擎对比定案（workflow 核心选型）**：

| 引擎 | 启动 | 内存 | ES 支持 | macArm | 判定 |
| --- | --- | --- | --- | --- | --- |
| rquickjs 0.12.1 | 快 8-16x（vs deno_core，windmill 实证） | 1.4M（script-bench-rs，M5 Max） | ES2023 全（modules / async generators / proxies / BigInt） | ✅ | **选定** |
| boa_engine 0.21.1 | 中 | 25.3M（18x） | 90%+ 不完整 | 不稳定 | 排除 |
| deno_core 0.408.0 | 慢（V8） | 大 | 完整 | ✅ 但重 | 排除 |

**rquickjs 的 tokio 桥接（开放问题 1 关闭）**：async-rt feature 原生集成——`AsyncRuntime` + `AsyncContext`，JS promise 可作 Rust future await（`promise.into_future()`），Rust async fn 直接注册为 JS 函数（`Func::from(Async(f))`），`ctx.spawn` + `rt.idle()` 驱动。workflow 的 agent()/pipeline() 原语直接成立，无需 spawn_blocking hack。

明确不引入：deno_core / boa（上述对比）、任何 Node 运行时、OpenSSL。

## 3. 上下文工程

### 3.1 缓存友好分段（peri 模式）

系统上下文分两段，保 provider prompt cache 命中（peri 实证 95-99%）：

- frozen 段：能力卡（工具说明 / 规则 / 角色定义），整会话不变
- boundary marker 分隔
- dynamic 段：goal 状态 / .agents 索引 / mrm 状态 / 环境，按需更新，经 mid-conversation system message 注入（OpenCode 模式）

### 3.2 渐进披露（Tool Search 模式）

- 常驻工具 ~12：exec / read / write / edit / glob / grep / task / todo / goal / workflow / webfetch / websearch
- 其余（LSP ops、MCP 工具、dev_server、scheduler 等）经 `tool_search` 按需发现并临时挂载——上下文只放当前需要的工具卡。

### 3.3 .agents/ OKF

- 项目知识目录（OKF bundle）：rules（type: rule，alwaysApply 注入）+ references（按需读取）+ index.md（渐进披露入口）+ 多层目录就近原则
- frontmatter 手写解析（~100 行，宽松消费：未知 type/字段不致命）
- 启动无 .agents 则跳过，不产生空段

### 3.4 会话与压缩

- JSONL 持久化（append-only）+ branch / fork / resume
- compaction：超阈值时 LLM 摘要 + 规则裁剪（工具大输出换路径引用）；压缩后 frozen 段与关键约束重注入
- workflow / subagent 的中间结果留在各自脚本与子上下文，主上下文只收最终值

## 4. 模型层

### 4.1 provider（全通用，自研薄层，不特殊化）

自研 provider 层（jcode 同款结构）：每 provider = endpoint + auth + SSE 的组合，~200-400 行；openai-compatible 端点一条通用实现覆盖长尾。不特殊化任何一家；用户当前恰好持有四订阅，仅意味着订阅探测当前有四条规则。

Claude OAuth contract（jcode OAUTH.md 实证，五要素缺一不可）：

1. Messages endpoint 带 `?beta=true`
2. `User-Agent: claude-cli/1.0.0`
3. `anthropic-beta: oauth-2025-04-20,claude-code-20250219`（双值）
4. 系统块第一行固定为 `You are Claude Code, Anthropic's official CLI for Claude.`
5. 内置工具名 allow-list 重映射（bash->Bash、read->Read、write->Write、edit->Edit、glob->Glob、grep->Grep、subagent->Agent、schedule->ScheduleWakeup、skill_manage->Skill），其余工具原名转发

### 4.2 订阅探测（kxen-auth）

通用机制：每个订阅一条探测规则（读官方 CLI 凭证存储 -> 比新鲜度（expires 大者优先）-> 导入 -> 过期刷新）。新增订阅 = 加一条规则。external 凭证文件**只读不动**（no move/rewrite/permission 变更，symlink 拒绝），首读需用户批准并记忆该批准（jcode 同款）。当前四条：

- Claude：Keychain `Claude Code-credentials`（keyring）或 `~/.claude/.credentials.json`；refresh `https://console.anthropic.com/v1/oauth/token`
- Codex：`~/.codex/auth.json`；refresh `https://auth.openai.com/oauth/token`；带 `originator: codex_cli_rs` + `chatgpt-account-id` 头，端点 `https://chatgpt.com/backend-api/codex/responses`（API key 模式走 `api.openai.com/v1/responses`，base 可覆盖）
- Grok：`~/.grok/auth.json`（issuer map 取 expires 最新）；xai API Bearer
- Kimi：`~/.kimi-code/credentials/kimi-code.json`；`https://api.kimi.com/coding/v1` Bearer

### 4.3 mrm（全局模型资源管理）

- 角色（config.toml [roles]）：thinking / planning / execution / review / research -> provider/model
- 调度：per-provider tokio Semaphore + RPM 滑窗 + 角色降级链
- 一切 LLM 调用与 subagent 派发经同一 acquire/release（RAII guard 自然释放）
- 状态摘要注入 planning 模型上下文，规划时自知限额

## 5. 编排层

### 5.1 goal（生命周期）

- 状态机：draft / queued / active / paused / complete / blocked / budget_limited / canceled
- 完成契约必填（objective + completionCriteria），预算三维（tokens / turns / wallClock）
- 阻塞三次规则：同一原因连续 3 轮才 blocked；terminal 当轮即可
- write-goal：GUI 内引导起草（AskUserQuestion 驱动：end state / proof / boundaries / loop / stop rule），确认后创建并激活
- score-based 验证（uira 模式）：完成判定逐条 proof 独立验证（命令/搜索/状态），全过才允许 complete
- 状态注入 dynamic 段（active / paused / blocked 渲染）

### 5.2 workflow（动态编排）

- 模型自主写 JS，rquickjs 沙箱执行：`agent(prompt, {role?})` / `pipeline(items, fn, {concurrency})` / `constraints()` / `phase(name)` / `args`
- agent() 经 tokio 桥到 agent loop
- 护栏：单次 run 调用上限 200、每调用超时、调用缓存（resume 回放）
- 触发：用户说 workflow / 任务规模明显超出单上下文时模型自主采用（能力卡在 frozen 段说明）

### 5.3 subagent

- 角色 agent 预设（model 绑定 + 权限预设 + prompt）：kxen-thinking / planning / execution / review / research
- task 工具派发，深度限制防递归；子代理权限从父级派生

### 5.4 loop 检测（rust-code 模式）

四层：exact（同调用重复）/ semantic（近似意图重复）/ output stagnation（输出不再变化）/ frequency churn（高频无效调用）。命中即中断并回写原因与换路建议。

### 5.5 hooks

- 事件：pre_tool_use / post_tool_use / notification / stop / session_start
- config.toml [hooks]，全部默认关闭；hook 是本地命令，与 exec 同过 safety 拦截

## 6. 工具层

### 6.1 命令调度（grok-build 源码实证）

1. **auto_bg**：前台命令阻塞预算 15s，超时自动转后台返回 task_id，agent 永不被长命令卡住；background 模式 timeout: 0 = 不限时
2. **完成通知代替 sleep/poll**：任务完成经事件主动通知；工具描述铁律 "do not poll or sleep-wait"
3. **任务三件套**：exec(background) -> task_id / task_output(id) / kill_task(id)
4. **静态快照 shell**：启动时一次性捕获 login shell 函数/alias 快照，每条命令 fresh shell 回放 + cd workdir；无状态污染、subagent 并发天然安全
5. **命令遮蔽**：`grep -> ugrep`、`find -> bfs`（brew 检测存在才启用，marker 门控 restore 不覆盖用户自定义函数）

输出纪律：100ms tick 节流 + 16KB/tick 增量上限 + 总量截断 + 大输出落会话文件只回路径。

### 6.2 dev server 管理

- `dev_server {command, workdir, ready?: {pattern?, port?, timeout_ms?}}`：后台启动 -> 阻塞等待就绪（输出匹配 `listening|ready|started|watching|serving|compiled` 或端口可达）-> 返回 `{task_id, url, pid}`；就绪失败回错误 + 日志尾部
- `restart_task(id)`：同配置重启（id 不变）
- `list_tasks()`：全部后台任务状态表（命令 / 状态 / uptime / 端口 / 输出尾部），agent 与 GUI 共用
- `monitor`：长运行脚本行级事件流（persistent 会话级，速率限制；提醒 `grep --line-buffered`）
- 健康检查：dev_server 任务每 30s 探测端口，失连即发事件
- GUI 后台任务页：状态灯 / 命令 / 端口 / uptime / 日志尾部 + 停止 / 重启

### 6.3 exec 工具

- `exec(type: zsh|bash|fish, path, command, timeout?, background?)`：type 必填（迫模型先想环境）
- 方言校验器：fish 无 export（用 set -x）、zsh 数组 1-index 等，命中即拒绝 + 纠正文案
- 与 safety 同层拦截；走 6.1 的静态快照 shell 与 auto_bg

### 6.4 读写删工具

- **read**：输出 `LINE#HASH` 锚点（ChunkFingerprint：行内容 hash + chunk 指纹，上方编辑不影响下方锚点）；截断 2000 行 / 长行 2000 字符
- **edit 双模式**：锚点模式 `edits: [{anchor, new_text}]`（优先，无歧义）+ 兼容模式 `old_string/new_string + expected_replacements`
- **免强制 read-before-edit**：会话内文件状态跟踪（path -> mtime + size + 锚点快照）；读过且未外部变更直接 edit，有变更仅重读相关段
- **失败自愈**：锚点失配自动 `find_shifted`（有界窗口找回），返回实际行内容与新锚点，不补 Read 轮
- **write**：新文件直写；覆盖已存在文件同 edit 免读规则；覆盖前自动快照（会话级 undo）
- **delete = trash**：exec 遮蔽 `rm` -> `trash`（/usr/bin/trash，macOS 14+ 自带；trash crate v5.2.6 备选），一切删除可恢复；write/edit 删除类操作同走 trash

### 6.5 safety（执行层硬拦截）

- 规则族 F1-F5：毁系统（/ /System /usr 等）/ 毁用户目录（~、~/Documents、凭证目录）/ 删 .git / 数据与基础设施毁灭（dropdb、terraform destroy、kubectl delete ns）/ 批量失控（无目标递归删、未求值变量）
- 实现：RegexSet（OnceLock 预编译）+ &str 切片扫描，热路径零分配；命中返回结构化错误（ruleId + reason + suggestion）
- 三层拦截：exec 命令文本、write/edit 路径守卫（resolve 后比对）、防绕过（bash -c / eval 递归评估）
- trash 降档：trash 的删除按「可恢复」记 approval 而非 forbidden；.git / 系统路径的 trash 仍 forbidden
- 不做内容级风控：逆向 / 破解 / 外挂类请求不拦截，防护只在执行层

### 6.6 LSP / MCP

- LSP：原生 auto-detect（语言服务器按项目类型自动接入），ops 经 Tool Search 披露
- MCP：.mcp.json 自动探测；工具列进 Tool Search 而非全量进上下文

### 6.7 worktree 隔离

- 批量迁移 / 并行修改（workflow pipeline 常规形态）：git worktree 隔离执行，完成 diff 回主树，并行 subagent 零冲突

## 7. 性能与安全纪律（无 Clone 原则）

- hot path 零分配：safety 扫描、SSE 解析、diff 渲染只用 `&str` / `&[u8]` 切片；共享字符串 `Arc<str>`；消息部件 `Box<[T]>`
- Regex/RegexSet 全部 OnceLock 预编译；HTTP client 全局单例
- 事件流零拷贝：tokio broadcast；大输出只传路径
- release profile：`opt-level = 3`、`lto = "thin"`、`codegen-units = 1`、`strip = true`、`panic = "abort"`
- 安全：Tauri capabilities 最小授权；凭证只读 Keychain / 0600 文件；exec 经 safety 硬拦截；无遥测零上传；updater 仅官方 Releases 签名包

## 8. GUI

**前端栈（2026-07-21 定案）**：

- **SolidJS**：UI 框架。无 VDOM 细粒度 signals，会话流式渲染（消息增量）天然匹配；runtime ~7KB，与 < 80MB / < 500ms 首绘目标一致
- **vite-plus（vp）**：统一工具链（dev / build / check / test 一体，含 oxlint + oxfmt + Vitest + Rolldown），替代裸 Vite；`vp dev` 开发、`vp build` 生产、`vp check` 检查
- Tailwind CSS v4：样式（构建时 purge，产物极小）
- Kobalte：Solid 无头组件（对话框 / 菜单 / 弹层）
- marked + mermaid：markdown 与图渲染
- shiki（marked-shiki）：代码高亮
- 自写极简 hash router（~50 行；单页几个视图，不引路由库）

页面：会话列表 / 会话视图（流式渲染）/ 角色与模型选择 / goal 面板 / 后台任务页 / doctor 状态页（凭证与环境自检）。

Rust -> 前端 events：`llm://delta`、`tool://call`、`task://update`、`goal://update`；前端 -> Rust commands：send_message / dispatch_task / goal_action / task_action / doctor。

目录选择：Rust read_dir 经 command 直返（无 HTTP 层）。

## 9. 里程碑（0 -> 1，每个可验证）

| 里程碑 | 内容 | 验证 |
| --- | --- | --- |
| M0 | workspace + Tauri 空窗 + kxen-auth 四源读取 + doctor 状态页 | 状态页显示四家凭证状态 |
| M1 | kxen-llm 单 provider（xai Bearer）+ 流式到 GUI | GUI 发消息收流式回复 |
| M2 | agent loop + 命令调度（快照 shell + auto_bg + 任务三件套 + dev_server）+ exec/读写删 + safety | 改文件 + rm -rf / 被拦 + 长命令自动后台化 + dev server 起停/就绪/重启可演示 + rm 实际进回收站 |
| M3 | 订阅全接入 + mrm 角色路由 + subagent | 四家各一次真实调用；角色 agent 派发 |
| M4 [DONE] | goal 全生命周期 + write-goal + workflow（rquickjs）+ loop 检测 | 已实测：状态机流转 + 预算/阻塞升级；write-goal 全链路（kimi create->activate->验证->complete）；workflow 并行编排（phase 流式 + Promise.all 双子代理）；loop 检测真实环境两次触发（exact/stagnation） |
| M5 | .agents/OKF + Tool Search + hooks + worktree + 签名 dmg | 已实测：OKF 注入 0 工具调用复述规则；tool_search 挂载 todo 调用成功；hooks 阻断 cowsay；worktree 隔离主树零接触；dmg 构建中 |

## 10. 明确不做

- Windows / Linux / Intel Mac
- CLI / TUI / daemon / HTTP API / 插件市场
- 内容级提示词风控
- 沙箱（safety 拦截代替；macOS Seatbelt 备查）
- 移动端（Tauri iOS 留未来选项）
- 遥测 / 会话分享 / 云同步

## 11. 开放问题

无。全部选型已冻结（2026-07-21 用户确认）。

## 附录 A：优点收纳矩阵

| 维度 | 最佳来源 | kxen 采纳 |
| --- | --- | --- |
| 形态 | Tauri（自定） | 纯 GUI app，仅 Apple Silicon |
| 性能 | jcode | < 20MB / < 80MB / < 500ms 首绘 |
| 命令调度 | grok-build（源码实证） | auto_bg / 完成通知 / 任务三件套 / 静态快照 shell / 命令遮蔽 |
| dev server | grok-build + 自定 | 就绪等待 / restart / list / 健康检查 / GUI 任务页 |
| 编排 | Claude Code | Dynamic Workflow（rquickjs 执行模型写的 JS） |
| 目标管理 | Kimi Code | goal 生命周期 + write-goal 契约 + score 验证 + 注入 |
| 子代理 | Claude Code + OpenCode | 角色化预设 + task 派发 |
| 模型调度 | 自定（analysis/03） | mrm 并发 / RPM / 降级 / 状态注入 |
| provider | OpenCode + jcode | 自研薄层（jcode 同款）+ openai-compatible 通用 + 订阅探测规则机制 |
| context 工程 | OpenCode + peri + DCP | frozen/dynamic 分段 + boundary marker + mid-conversation 注入 |
| 渐进披露 | peri | Tool Search（常驻 ~12，其余按需） |
| 编辑工具 | grok-build hashline + pi_agent_rust | ChunkFingerprint 锚点 + 双模式 edit + 免强制先 Read + find_shifted 自愈 |
| 删除语义 | grok-build + macOS | rm -> trash 遮蔽，删除可恢复 |
| 命令策略 | Codex execpolicy + 自定 | safety F1-F5 硬拦截，无内容风控 |
| loop 检测 | rust-code | 四层检测，命中中断 |
| goal 验证 | uira | score-based 逐条 proof |
| 会话 | pi_agent_rust + OpenCode | JSONL + branch/fork/resume + compaction |
| hooks | Claude Code + OpenCode | 事件钩子，默认关闭可选开启 |
| LSP/MCP | OpenCode + OMP | auto-detect + Tool Search 披露 |
| worktree | OpenCode | git worktree 隔离并行安全 |
| .agents/OKF | 自定（design/09） | rules 注入 + 索引渐进披露 + 多层就近 |
| scheduler | grok-build | cron 式定时任务（M5 后增强） |
| spec 驱动 | claurst | docs/ 调研即 spec（本方法论） |

## 附录 B：参考实现（源码对照阅读库，SelfCode 下）

| 项目 | 借鉴点 |
| --- | --- |
| xai-org/grok-build（21k stars，Rust） | 命令调度五机制源码实证；工具描述工艺；xAI OAuth |
| MoonshotAI/kimi-code | goalService / goalInjection / skillCatalog；write-goal skill 原文 |
| 1jehuang/jcode（9.6k stars，Rust 93.5%） | 性能基准；OAUTH.md 与 auth 模块；轻结构 |
| KonghaYao/peri（Apache 2.0） | Goal/Workflow 的 Rust 形态；cache 命中设计；Tool Search |
| Kuberwastaken/claurst（GPL-3.0，仅方法） | spec 驱动开发 |
| Dicklesworthstone/pi_agent_rust | 自写 SSE parser；零 unsafe；hashline 思路 |
| openai/codex（codex-rs） | workspace 领域划分；execpolicy |
| junhoyeo/uira | score-based goal 验证；macOS Seatbelt 备查 |
| anomalyco/opencode（fork 留存） | provider 广度；system-context（Context Source/Epoch）；agent 配置化；worktree |
