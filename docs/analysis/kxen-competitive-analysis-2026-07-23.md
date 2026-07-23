# kxen 与同类 coding agent harness 功能对比评估

- 报告日期：2026-07-23
- 基线对象：kxen（本地仓库 `file:///Users/xiaobai/Code/SelfCode/kxen`，源码逐文件盘点）
- 对比对象：Claude Code、OpenAI Codex、Gemini CLI、OpenCode、Crush、Cline/Roo、Goose、Conductor、Vibe Kanban（共 9 个产品）
- 证据口径：kxen 事实一律带 `file:line`（源码实读，未跑真实 app，未跑编译，行为路径推断处标 UNKNOWN）；竞品事实带来源 URL（官方文档 + GitHub + 少量第三方，均在文中标注）；不确定处标 UNKNOWN，不臆断

---

## 1. 执行摘要

### 1.1 kxen 定位

kxen 是一个 **aarch64-apple-darwin 专精的原生桌面 coding agent harness**（Tauri 2.x + SolidJS 前端，前后端走内嵌本地随机端口 WebSocket + JSON-RPC 3.0，`src-tauri/src/ws/mod.rs:46-47`）。它不自研模型，而是**寄生四家官方 CLI 已落盘的订阅凭证**（Claude / Codex / Grok / Kimi），做多账号池化 + 角色路由 + 并发限流调度（`src-tauri/src/llm/mrm.rs`）。与 9 个竞品相比，kxen 的产品形态最接近 Conductor（原生 macOS GUI），但 Conductor 只封装他人 harness，kxen 是**自研 agent loop + 原生 GUI 一体**，这一组合在 10 家里是少数。

kxen 的差异化集中在四处：MRM 全局资源调度（唯一的 harness 级资源调度器）、四层 loop 检测（最完整的自治可靠性防护）、QuickJS 沙箱 workflow（与 Claude Code Dynamic workflows 并列的唯二原生脚本编排引擎）、OKF 单规范统一知识系统。短板集中在：无 MCP、无 OS 沙箱、无请求重试、无上下文压缩、goal 自治闭环未接线、非开源且仅单平台。

### 1.2 最重要的 5 个差距（按严重度）

1. **P0 无 MCP（Model Context Protocol）**：全库 grep `mcp/Mcp/MCP` 零命中（`grep mcp src-tauri/src` 复核），design 6.6 整节为 planned。9 个竞品**全部**支持 MCP，Goose 生态甚至完全建立在 MCP 之上。这切断了第三方工具/数据源接入的行业标准通道。对标 Claude Code / Codex / OpenCode / Goose。
2. **P0 无请求重试/退避 + 无 OAuth token 主动刷新**：HTTP 失败即产出 `Delta::Error` 当轮终止，无 429/5xx 退避、不换账号（`src-tauri/src/agent/agent_loop/run.rs:115-119`，llm 层 grep retry/backoff 零命中）；token 刷新未实现，委托官方 CLI，官方 CLI 未运行则过期 token 无法复活（`src-tauri/src/auth/probe.rs:57-60`），且 doctor 文案 "will refresh on next call" 与实现不符（`src-tauri/src/doctor.rs:29,42`）。对标 Claude Code（`RETRY_WATCHDOG`）、OpenCode（`RETRY_MAX_ATTEMPTS=10` + 退避封顶，均为生产事故后加固）、Crush（到期前 30s 自动刷新）。
3. **P0/P1 无上下文压缩（compaction）**：全库 grep `compact/compaction` 零业务命中（scan-docs-intent 第19节，design 3.4 标 planned）；ctx 窗口硬编码 200k 仅在状态栏显示百分比（`src-tauri/src/ws/settings.rs:213`）。长会话必然撞上下文上限且无任何缓解手段。对标 Claude Code（`/compact` + 自动压缩 + 压缩后重注入 CLAUDE.md）、Gemini CLI（50% 阈值自动压缩 + 二次校验轮）、OpenCode（`compaction.auto` 默认开）、Codex（PreCompact/PostCompact hook）、Goose（tool pair summarization）、Crush（自动摘要）。
4. **P0 无 OS 级沙箱 + 无 ask-user 审批档**：无 Seatbelt/bubblewrap/容器/网络隔离；`Verdict` 只有 `Allow`/`Deny`/`Recoverable` 三档（`src-tauri/src/tools/safety/rules.rs:8`），无交互审批中间态，`push --force`/`reset --hard`/`sudo` 等不在任何规则表内按 Allow 静默放行。对标 Claude Code / Codex / Gemini（三家均有 OS 沙箱 + 网络隔离 + 多档审批）。
5. **P0 goal 自治闭环未接线运行路径**：goal 状态机 + 三维预算 + 阻塞三次规则实现完整且有单测，但 `record_turn` 全库唯一 production 调用点是 `goal_rpc.rs:66`（RPC 手动触发），agent_loop 主循环与前端均不调用（grep 验证）。即"防失控"在预算维度实际未落地。对标 Codex `/goal`（运行时驱动 budgetLimited/blocked）。

### 1.3 最重要的 3 个独有优势

1. **MRM 全局资源调度 + 多订阅账号池化路由**：`RoleBinding`（provider/model/fallback/account）+ `resolve()` 按 role_chain 降级 + per-provider/全局 semaphore + 60s RPM 滑窗 + 账号钉选/字典序轮转（`src-tauri/src/llm/mrm.rs:58-224`），是 10 家中唯一的 harness 级资源调度器。竞品要么无（Crush/Conductor/Vibe Kanban/OpenCode），要么明确否决（Goose issue #6615），要么只有账号侧 5h 窗口限额由服务端管（Claude/Codex）。这直接支撑"多套消费级订阅并存 + 按角色/任务分别路由到不同账号"，在各竞品调研中该能力普遍标 UNKNOWN。
2. **四层递进 loop 检测（运行时强制层）**：exact(3) -> semantic(5，数字不折叠) -> stagnation(8) -> churn(6)，真实接入 4 类运行路径（主 loop / subagent / team member / 顶层 llm_task），命中即软停写原因（`src-tauri/src/agent/loop_detect.rs:13-149`）。Claude Code 无统一 loop 防护层（官方承认），OpenCode 曾跨消息漏检重复 1827 次（根因 issue `anomalyco/opencode#25254` 截至 2026-07-23 仍 OPEN，见 7.3 时效性核实），Goose 有无声死循环 bug #7527。kxen 覆盖面最完整。
3. **OKF 单规范统一知识系统**：一份 frontmatter 超集 + 统一目录约定 + 单一 scan/render/store 管道，覆盖 rule/reference/skill/command/note/memory/history 全 7 类知识，双 scope（project 入 git / personal 跟人）；注入四态分级是引擎级行为而非提示词约定（`src-tauri/src/knowledge/render.rs:10-116`）。竞品普遍是"rules 文件 + 独立 memory + 独立 skills"多套割裂子系统（Claude Code 的 CLAUDE.md/MEMORY.md/skills 三分、Codex 的 AGENTS.md/Memories/Skills 三分）。

---

## 2. kxen 当前功能全景（按子系统）

状态口径：implemented = 代码 + 调用链完整；partial = 有实现但存在明确缺口；stub = 结构在但无实际行为；planned = 仅设计文档/枚举变体，代码零落地。

### 2.1 模型接入与订阅认证

| 功能                                                                                    | 状态        | 证据                                                                 |
| --------------------------------------------------------------------------------------- | ----------- | -------------------------------------------------------------------- |
| 四源官方凭证探测（Claude Keychain/文件、Codex JWT、Grok issuer-map、Kimi 秒转毫秒）     | implemented | `src-tauri/src/auth/probe.rs:26-31,66-108,139-259`                   |
| 新鲜度比较 + 30min 豁免窗 + 中毒值(10 年远期)自愈                                       | implemented | `src-tauri/src/auth/probe.rs:66-108,58-61`                           |
| Keychain 5s 超时保护（防未签名二进制 ACL 弹窗卡启动）                                   | implemented | `src-tauri/src/auth/probe.rs:35-42`                                  |
| 多账号池化（默认账号=裸 provider，命名账号=`provider:name`，钉选/字典序轮转/RPC 增删）  | implemented | `src-tauri/src/auth/credential.rs:51-82`                             |
| auth.json 原子写(tmp+rename) + 0600 权限                                                | implemented | `src-tauri/src/auth/credential.rs:91-104`                            |
| Anthropic OAuth 契约五要素 + 工具名双向重映射(exec<->Bash 等)                           | implemented | `src-tauri/src/llm/anthropic.rs:8-44`                                |
| OpenAI/Codex Responses API 双端点(订阅 chatgpt.com/backend-api/codex vs api.openai.com) | implemented | `src-tauri/src/llm/openai.rs:126-158`                                |
| xAI/Kimi openai-compatible 通用实现 + 自定义 provider 双协议(openai/anthropic)          | implemented | `src-tauri/src/llm/xai.rs`；`src-tauri/src/client.rs:66-81`          |
| 模型目录三级源(内存->磁盘->静态兜底) + 24h TTL 惰性后台刷新 + models.dev                | implemented | `src-tauri/src/llm/catalog.rs:61-119`                                |
| 端点实时模型清单拉取(含自定义双协议，kimi 不支持则静默回退)                             | implemented | `src-tauri/src/llm/models.rs:1-74`                                   |
| 订阅活性 ping 校验(真发一条 ping，20s 判活)                                             | implemented | `src-tauri/src/llm/verify.rs`                                        |
| 自研 SSE 帧解析器 + 工具调用分片累积                                                    | implemented | `src-tauri/src/llm/sse.rs`；`src-tauri/src/llm/tool.rs`              |
| 图片输入(base64，三 provider 各自块格式)                                                | implemented | `src-tauri/src/llm/types.rs:29-40`                                   |
| OAuth token 主动刷新(refresh grant)                                                     | planned     | grep `grant_type/refresh_token=/token_endpoint` 零命中；委托官方 CLI |
| 请求失败重试/退避/429 特判/换账号                                                       | planned     | `src-tauri/src/agent/agent_loop/run.rs:115-119`，llm 层 grep 零命中  |
| 公网图片 URL 输入                                                                       | planned     | 全链路仅 base64                                                      |

### 2.2 代理编排与自治

| 功能                                                                                             | 状态                | 证据                                                                                |
| ------------------------------------------------------------------------------------------------ | ------------------- | ----------------------------------------------------------------------------------- |
| 5 硬编码 subagent 预设(thinking/planning/execution/review/research) + 权限画像                   | implemented         | `src-tauri/src/agent/subagent.rs:80-90`                                             |
| 只读角色不含写工具(编译期单测强约束 readonly_roles_cannot_write)                                 | implemented         | `src-tauri/src/agent/subagent.rs:174-184`                                           |
| subagent max_turns 硬编码 6、不能派孙代理、角色不可用户自定义                                    | partial             | `src-tauri/src/agent/subagent.rs:94-159`(无角色文件加载路径)                        |
| QuickJS 沙箱 workflow(10min/32 agent/64MB/1MB 栈，agent/phase/log 三 API + CONSTRAINTS 只读全局) | implemented         | `src-tauri/src/agent/workflow.rs:19-167`                                            |
| ultracode/ultraplan/ultrareview(命令入口 + 提示词剧本，无服务端强制分解/校验)                    | partial             | `src-tauri/src/agent/prompt.rs:65-89`；`src-tauri/src/agent/commands.rs:16-25`      |
| workflow resume/journal(崩溃/取消从头重跑)                                                       | planned             | 无缓存/回放代码痕迹                                                                 |
| Goal 8 态状态机 + 三维预算 + 阻塞三次规则(完整 + 单测)                                           | implemented         | `src-tauri/src/core/goal.rs:5-174`                                                  |
| goal record_turn 接线主循环(预算/阻塞运行时自动驱动)                                             | partial             | 仅 `goal_rpc.rs:66` RPC 可达，agent_loop/前端不调用(grep 验证)                      |
| GoalUpdate 事件 publish                                                                          | stub                | `src-tauri/src/core/event.rs:10` 变体定义但无 publish 调用点                        |
| Agent Teams(spawn/inbox/plan 审批门/observer 抄送/依赖解锁 task list/hook 否决)                  | implemented         | `src-tauri/src/agent/team/*`                                                        |
| Team 状态跨进程存活                                                                              | partial             | `TeamManager::new` 构造时 `remove_dir_all(root)` 清空(`manager.rs:24-28`)，重启清零 |
| Cron 调度(5 字段，15s 轮询驱动)                                                                  | implemented(纯内存) | `src-tauri/src/core/schedule.rs`，重启丢失(`schedule.rs:18`)                        |
| Agent 活动注册表(3 类 + 200 条转录环形缓冲)                                                      | implemented(纯内存) | `src-tauri/src/agent/agent_loop/activity.rs`，不持久                                |
| 取消令牌三检查点级联 + 子代理继承 + 终态必落库                                                   | implemented         | `src-tauri/src/agent/cancel.rs`；`run.rs:184-187`                                   |

### 2.3 工具执行与安全

| 功能                                                                                      | 状态                       | 证据                                                                                                |
| ----------------------------------------------------------------------------------------- | -------------------------- | --------------------------------------------------------------------------------------------------- |
| exec 快照 shell(每命令全新进程回放 login+rc 的 alias/function，无状态并发安全)            | implemented                | `src-tauri/src/tools/shell.rs:52-80`                                                                |
| auto_bg 15s 自动前台转后台 + task 注册表(start/output/kill/list/restart)                  | implemented                | `src-tauri/src/tools/exec.rs:14,86-113`；`src-tauri/src/tools/task.rs`                              |
| dev server 就绪门(关键词 + 端口探测 + URL 正则解析 + 30s 健康检查 + restart)              | implemented                | `src-tauri/src/tools/dev_server.rs:65-134`                                                          |
| Fish 快照捕获                                                                             | partial                    | `shell.rs:55` 循环仅 [Zsh, Bash]；fish 二进制硬编码 Intel 路径 `/usr/local/bin/fish`(`shell.rs:21`) |
| 进程组/进程树 kill(SIGTERM->SIGKILL 升级)                                                 | partial                    | `task.rs:92` 仅 `child.kill()` 单进程 SIGKILL，孙进程泄漏                                           |
| F1-F5 破坏命令语义分类器(含 nested bash -c 递归解包 + sudo 前缀剥离 + 变量未求值检测)     | implemented                | `src-tauri/src/tools/safety/eval.rs`；`rules.rs:20-86`                                              |
| 命令解析防绕过(`;`/`                                                                      | `/`&&` 分段 + nested 提取) | partial                                                                                             | 未处理 ` |     | `/换行/反引号/`$()`(`eval.rs:39`)，无沙箱兜底即安全洞 |
| OS 级沙箱(Seatbelt/bwrap/容器/网络隔离)                                                   | planned                    | 全库无相关调用                                                                                      |
| approval/ask-user 交互审批档                                                              | planned                    | `Verdict` 仅 3 变体(`rules.rs:8`)，docs 三档设计未落地                                              |
| 项目信任门(加载 .kxen/config.toml hooks 与 .agents/ 前的信任确认)                         | planned                    | `config.rs:118-130` 自动 merge 项目级，无信任机制                                                   |
| read hashline 锚点(行号#4位hex) + edit 双 mode(anchors 自愈±20 行 / match)                | implemented                | `src-tauri/src/tools/fs_tool.rs:89-207`；`hashline.rs`                                              |
| delete/session 删除走系统废纸篓(可 Finder 恢复) + write 前 .kxen-bak 备份                 | implemented                | `src-tauri/src/tools/fs_tool.rs:221-246`                                                            |
| rm->trash 透明遮蔽(shell 层内联函数)                                                      | implemented(仅 rm)         | `src-tauri/src/tools/shell.rs:83`；grep->ugrep/find->bfs 未实现                                     |
| glob(尊重 .gitignore) + grep(regex，单文件 512KB 上限)                                    | implemented                | `src-tauri/src/tools/search.rs:22-96`                                                               |
| webfetch(正则剥 HTML 非 DOM，50k 字符 cap，deferred)                                      | implemented                | `src-tauri/src/tools/webfetch.rs`                                                                   |
| websearch 工具                                                                            | planned                    | 全库不存在(design 3.2 承诺，scan-docs-intent 第16节)                                                |
| hooks(pre_tool_use 阻断 + post_tool_use 仅 warn + team 专属 task_completed/teammate_idle) | partial                    | `src-tauri/src/tools/hooks.rs`；notification/stop/session_start 未实现                              |

### 2.4 会话与 UI

| 功能                                                                                               | 状态        | 证据                                                         |
| -------------------------------------------------------------------------------------------------- | ----------- | ------------------------------------------------------------ |
| 原生 textarea composer(@//#三触发 + 每会话草稿 + 粘贴占位 + WebKit IME 锁窗 + 内嵌模型/角色选择器) | implemented | `src/components/composer/*`                                  |
| 语音 PTT 双引擎(Apple Speech.framework 本地流式 partial + 云转写降级 + Wispr 双轨)                 | implemented | `src-tauri/src/voice/*`                                      |
| SessionTree(directory 分组 + 拖拽排序 + 内联重命名 + 运行态脉冲点)                                 | implemented | `src/components/SessionTree.tsx`                             |
| SessionTree 单组超 5 条展开                                                                        | partial     | `SessionTree.tsx:148` 仅显数量                               |
| sessionFork(从消息复制前缀历史) + 编辑重发 fork + rerun                                            | implemented | `src-tauri/src/core/session.rs:175-191`                      |
| checkpoint/rewind 时间旅行(代码+对话整体回退到某轮)                                                | planned     | 仅 fork + 单文件 .kxen-bak + 内存 diff 快照(不可 revert)     |
| CommandPalette(Cmd/Ctrl+K 三路混合搜索 command/session/model)                                      | implemented | `src/components/CommandPalette.tsx`                          |
| NotificationCenter(应用内铃铛 + 未读角标，5s 轮询)                                                 | implemented | `src/components/NotificationCenter.tsx`                      |
| OS 级/跨会话桌面推送通知                                                                           | planned     | `src/lib/delta.ts:41-42` 丢弃非活跃会话事件                  |
| DockWorktree(list/create/remove) + 子代理 worktree 隔离派发                                        | implemented | `src/components/DockWorktree.tsx`；`execute.rs:238-253`      |
| 并行 workspace 看板(以 worktree 为并行单元的中心视图)                                              | planned     | worktree 仅 dock 面板 + 子代理隔离选项                       |
| 改动快照面板(agent 改动 diff，独立于 git status) + 后台任务 dock + goal dock                       | implemented | `src/components/Dock.tsx`；`src-tauri/src/tools/snapshot.rs` |
| ThinkingOrb 四态 canvas 动画(a11y/省电/降级) + 流式安全 markdown/mermaid 管线                      | implemented | `src/components/ThinkingOrb.tsx`；`src/lib/markdown.ts`      |
| Settings「用量与统计」节                                                                           | stub        | `src/pages/Settings.tsx:141-145` 纯静态文案                  |
| RoutingSection 降级链 fallback 编辑                                                                | partial     | `RoutingSection.tsx:80-84` 只读展示，UI 无编辑入口           |

### 2.5 知识与记忆

| 功能                                                                                             | 状态        | 证据                                                      |
| ------------------------------------------------------------------------------------------------ | ----------- | --------------------------------------------------------- |
| OKF 双 scope(project/personal) x 7 类 kind 统一一棵树 + 统一 frontmatter 超集解析                | implemented | `src-tauri/src/knowledge/mod.rs:18-123`；`parse.rs:7-114` |
| 注入四态分级(全文 Rules / 全文 Notes&memory / 索引 Reference-History-Command / 清单 Skills)      | implemented | `src-tauri/src/knowledge/render.rs:10-116`                |
| globs 条件激活 + mid-turn 刷新(本轮文件集变化即重建 system prompt)                               | implemented | `render.rs:81-89`；`run.rs:76-82`                         |
| 多层就近 AGENTS.md(沿 involved 文件目录向上逐级找)                                               | implemented | `render.rs:92-116`                                        |
| knowledge 工具(模型可写 add/list/remove，5 类 note，同 slug 覆盖)                                | implemented | `src-tauri/src/agent/agent_loop/execute.rs:85-105`        |
| 会话删除兜底蒸馏(尾部 12000 字符喂 LLM 提炼 0-5 条 note，best-effort)                            | implemented | `src-tauri/src/knowledge/distill.rs:57-87`                |
| skill 渐进披露(discovery/load + needs 懒加载 + 递归 cap 3 + 同参去重 + disable_model_invocation) | implemented | `src-tauri/src/agent/skills.rs`；`execute.rs:209-237`     |
| move_entry 跨 scope 晋升/降级                                                                    | implemented | `src-tauri/src/knowledge/store.rs:90-100`                 |
| 第三方规则格式互操作(CLAUDE.md/GEMINI.md/.cursorrules 等)                                        | planned     | `scan.rs` 仅 AGENTS.md 分支                               |
| 记忆动态检索(按相关性/tag/usage)                                                                 | planned     | notes/memory 一律全文注入(单条截断 500 字符)              |
| injection_preview 感知会话文件                                                                   | partial     | `ops.rs:100` 固定 involved=[]，看不到 glob 动态命中       |

### 2.6 生态与扩展性

| 功能                                    | 状态        | 证据                                                                                             |
| --------------------------------------- | ----------- | ------------------------------------------------------------------------------------------------ |
| MCP(client/server/.mcp.json 探测)       | planned     | 全库 grep 零命中，design 6.6 planned                                                             |
| LSP/代码智能(诊断/引用/重命名)          | planned     | 全库 grep `lsp/Lsp/LSP` 零命中，design 6.6 planned                                               |
| 插件 / marketplace 打包分发             | planned     | 无插件系统、无市场                                                                               |
| SDK / 编程接口                          | planned     | `src-tauri/src/bin` 不存在，唯一通道是内嵌 WS 面向自身前端                                       |
| headless / CI(CLI 子命令/GitHub Action) | planned     | main.rs 仅 run/ws_port 两个 command                                                              |
| 定价                                    | implemented | BYO 订阅探测，零计费零转售                                                                       |
| 开源协议                                | 无          | 根目录无 LICENSE，`package.json` `"private": true`，个人 repo `https://github.com/StringKe/kxen` |
| 跨平台                                  | 无          | 仅 aarch64-apple-darwin，无 Windows/Linux 分支                                                   |

---

## 3. 竞品概览表（9 个产品）

| 产品         | 形态                                                                           | 模型接入                                                                                    | 编排                                                                                      | 开源与定价                                                                                  |
| ------------ | ------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| Claude Code  | 终端 CLI + VS Code/JetBrains 扩展 + Desktop App + Web + Cloud                  | 单模型族(Claude)多 provider(Bedrock/Vertex/Foundry/gateway)，env var 优先于订阅 OAuth       | 四层(subagent/agent view/teams 实验/dynamic workflows 上限 1000 并发 16)                  | CLI 闭源；SDK 双协议(TS 非 MIT / Python MIT)；Pro $17-20 / Max $100-200 / Team / Enterprise |
| OpenAI Codex | Rust CLI(Ratatui TUI) + IDE 扩展 + ChatGPT 桌面 App + cloud                    | 单模型族(GPT-5.6)+实验 Bedrock；Sign in with ChatGPT OAuth / API key                        | 原生 subagent V2(可配 model/reasoning/concurrency) + Agents SDK(经 MCP) + cloud best-of-N | Apache 2.0(`github.com/openai/codex/blob/HEAD/LICENSE`)；ChatGPT 订阅 / API key             |
| Gemini CLI   | 纯终端 CLI(IDE 靠 Companion/ACP)                                               | 仅 Google 生态(Gemini/Vertex)；Google OAuth / API Key / Vertex 三选一                       | 本地 subagent + A2A 远程 agent；无声明式 workflow 引擎                                    | Apache 2.0；CLI 免费，付费在 Google AI Pro/Ultra 或 API 按量                                |
| OpenCode     | TUI + 桌面 App(Beta) + VS Code/Zed 扩展，client/server                         | 75+ provider(AI SDK+Models.dev)；各 provider 独立 OAuth，Claude Pro/Max 已被 Anthropic 封锁 | Agent primary/subagent + 后台 subagent(实验，需 env flag)；无原生 workflow 引擎           | MIT；软件免费，Zen/Go/Black 托管网关可选                                                    |
| Crush        | 终端 TUI(Bubble Tea)，无 GUI                                                   | 近 30 provider + Catwalk 元数据自动同步；`crush login hyper/copilot` 两条 OAuth             | 仅 2 硬编码 subagent(agent/agentic_fetch)，不可自定义；无 workflow 引擎                   | FSL-1.1-MIT(非 OSI，2 年后转 MIT)；免费，成本来自 provider(Hyper 订阅可选)                  |
| Cline/Roo    | Cline: VS Code+CLI+SDK+Kanban+菜单栏；Roo: VS Code(2026-05-15 停运转 Zoo Code) | 15+ provider(含 openai-codex 复用 ChatGPT)；Cline OAuth(WorkOS)+ClinePass+BYOK              | Cline 三层(只读 subagent/会话 sub-agent/跨会话持久 Teams)；Roo Orchestrator+Modes         | Cline Apache 2.0；ClinePass $9.99/mo / BYOK                                                 |
| Goose        | Electron 桌面 + CLI + REST server(goosed)                                      | 15-30+ provider；不自做 Anthropic OAuth(ToS 顾虑)，走 CLI/ACP pass-through                  | subagent + sub-recipe(均不可再嵌套)；worker 硬顶 10 不可配；无脚本引擎(维护者否决)        | Apache 2.0(捐 Linux Foundation AAIF)；免费，BYO provider                                    |
| Conductor    | 原生 macOS 桌面 App(封装 CC/Codex/Cursor/OpenCode)                             | 透传底层 harness，BYO 订阅，不计费不转售                                                    | 核心单元=workspace(=worktree+分支)；无角色化 subagent/无脚本引擎                          | 闭源(无公开 repo)；应用免费，Cloud beta 定价未公开                                          |
| Vibe Kanban  | 本地 Web 服务 + 后补 Tauri 壳                                                  | 透传 10+ 种 agent CLI，不管凭据                                                             | workspace+session 多实例 + 每 task attempt 独立 worktree；无角色化/无脚本引擎             | Apache 2.0(Bloop 2026-04 关停转社区)；本地免费，Cloud 曾 $30/user/mo 已关停                 |

---

## 4. 七维度逐项对比

### 4.1 维度一：模型接入与订阅认证

**kxen 现状**：4 家内置订阅 provider + 双协议自定义通道；不自做 OAuth 流（无 PKCE/authorize/token 端点，grep 空），全部读官方 CLI 已落盘凭证导入 auth.json；多账号池化 + MRM 角色路由/降级链；token 刷新未实现（委托官方 CLI）；请求重试未实现；强制流式无非流式路径。

| 产品        | 内置 provider 广度                          | 订阅 OAuth 复用方式                                                 | Catalog                                | Retry/退避                                                       | 运行时限流调度                        |
| ----------- | ------------------------------------------- | ------------------------------------------------------------------- | -------------------------------------- | ---------------------------------------------------------------- | ------------------------------------- |
| kxen        | 4 家 + 双协议自定义                         | 读官方 CLI 凭证导入，不自做 OAuth；多账号池化                       | models.dev 三级源 + 24h TTL + 静态兜底 | 无(失败即终止本轮)                                               | MRM 并发+RPM 滑窗+角色降级链          |
| Claude Code | 单模型族多 provider(Bedrock/Vertex/Foundry) | 原生订阅 OAuth `/login`，但 env var 优先(易误计费)                  | 内置模型别名，无独立目录抽象           | `RETRY_WATCHDOG` 对 429/529 无限重试(退避≤5min)，其他默认 300 次 | 无统一限流层；账号侧 5h 窗口          |
| Codex       | 单模型族 + 实验 Bedrock                     | 原生 Sign in with ChatGPT，凭证 file/keyring/auto                   | `/model` 内置档位                      | goal 态 usageLimited/blocked 防空耗                              | 5h 滚动窗 + 周限额(服务端)            |
| Gemini CLI  | 仅 Google 生态                              | Google OAuth / API Key / Vertex 三选一                              | 内置列表 + Auto 路由                   | ModelAvailabilityService 自动降级 fallback 链                    | 每用户每日配额(按认证分层)            |
| OpenCode    | 75+ provider                                | 各 provider 独立 OAuth；Claude Pro/Max 已被封；含 Zen/Go/Black 网关 | Models.dev 统一抽象(与 kxen 同源)      | `RETRY_MAX_ATTEMPTS=10` + 60s 退避封顶(生产事故后加固)           | compaction token 阈值；无美元硬预算   |
| Crush       | 近 30 + Catwalk 自动同步                    | `crush login hyper/copilot` 两条；无消费者订阅登录                  | Catwalk 开源仓库自动同步               | UNKNOWN(未见退避细节，有流层重复执行 bug #3108)                  | 无(仅工具循环检测)                    |
| Cline/Roo   | 15+(含 openai-codex 复用 ChatGPT)           | Cline OAuth(WorkOS device flow)+ClinePass+BYOK                      | 依赖各 SDK，无独立目录                 | Cline `--retries`(默认3) + `--timeout`                           | 无 harness 级；hub 负载高时暂停调度   |
| Goose       | 15-30+                                      | 不自做 Anthropic OAuth(ToS 顾虑)，CLI/ACP pass-through              | 依赖 provider                          | 内建 `retry_manager`(网络超时/429/5xx/瞬时错误自动重试)          | 无内置 rate-limit(功能请求被否 #6615) |
| Conductor   | 透传底层 harness                            | BYO 订阅，完全透传不计费                                            | 无(透传)                               | 无(依赖底层)                                                     | 无(依赖底层账号额度)                  |
| Vibe Kanban | 透传各 CLI                                  | 完全下放各 CLI 自身 OAuth/key                                       | 无(透传)                               | 无                                                               | 无(依赖被编排 agent)                  |

来源：https://code.claude.com/docs/en/authentication ；https://developers.openai.com/codex/auth ；https://geminicli.com/docs/get-started/authentication/ ；https://opencode.ai/docs/providers/ ；https://env.dev/ai/opencode (Anthropic 2026-01-09 封第三方 Claude Pro/Max OAuth) ；https://charmbracelet-crush.mintlify.app/cli/login ；https://docs.cline.bot/getting-started/authorizing-with-cline ；https://block-goose.mintlify.app/concepts/agents (retry_manager) ；https://www.conductor.build/docs/reference/harnesses ；https://vibekanban.mintlify.dev/docs/cloud/authentication

**差距**：

- P0 请求重试/退避：kxen HTTP 失败即终止本轮，无 429/5xx 退避、无换账号。对标 Claude Code(RETRY_WATCHDOG)、OpenCode(RETRY_MAX_ATTEMPTS+封顶)、Goose(retry_manager)，均为成熟能力。
- P0 OAuth token 主动刷新：官方 CLI 未运行则 token 死；doctor 文案与实现不符。对标 Codex/Gemini/Cline/Crush(到期前 30s 自动刷新+singleflight)。
- P1 内置 provider 广度：仅 4 家，靠双协议自定义弥补，缺 Bedrock/Vertex/OpenRouter/Ollama 一等封装。
- P1 运行时错误后自动降级/换账号：MRM 降级链仅在选择模型/并发槽阶段生效，不感知运行时 HTTP 错误。对标 Gemini ModelAvailabilityService。
- P1 非流式/结构化输出：强制 stream:true，缺结构化 JSON 保证。对标 Codex SDK/Gemini output_schema。

**优势**：

- 多订阅账号池化 + 角色路由 + 降级链一体：竞品订阅 OAuth 普遍单账号单登录态（Codex/Gemini/Claude Code 官方文档均未见多账号并行路由，各标 UNKNOWN），kxen 的"多套订阅按角色/任务分别路由到不同账号"是全场唯一原生能力。
- 官方 CLI 凭证零迁移复用 + 新鲜度自愈：只读不动外部凭证文件、30min 豁免窗、中毒值自愈、Keychain 5s 超时防卡死。
- Anthropic OAuth 契约完整落地（五要素 + 工具名双向重映射）：Crush/OpenCode 因 ToS 已无法做 Claude 订阅复用。

> 风险提示（修正原判断）：kxen 只是 token 获取途径不同（读官方 CLI 落盘凭证），**请求侧仍以伪装 `claude-cli/1.0.0` UA + OAuth 契约五要素的第三方客户端身份直连订阅端点**（`src-tauri/src/llm/anthropic.rs:8-11`）。这与 OpenCode 被 Anthropic 封锁的用法、Goose 因 ToS 顾虑拒绝合并的伪造请求头方案（`github.com/aaif-goose/goose/issues/3647`）**同属一类**。kxen 并未"绕开"封锁与封号风险，此为实质性 ToS 风险，不应表述为设计优势。

### 4.2 维度二：代理编排与自治

**kxen 现状**：5 硬编码角色预设 + 权限画像；三条并行通路（workflow 扇出上限 32 / Agent Teams / worktree 隔离）；QuickJS 沙箱脚本编排；goal 8 态状态机（但 record_turn 未接线主循环）；MRM 资源调度；4 层 loop 检测。

| 产品        | subagent 角色化                                                                             | 并行编排                                                              | workflow 脚本化                                                         | goal 生命周期                                                                                       | 资源调度                                   | 自治可靠性                                                                                    |
| ----------- | ------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- | ----------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- | ------------------------------------------ | --------------------------------------------------------------------------------------------- |
| kxen        | 5 硬编码 + 权限画像单测；不可自定义，max_turns 固定 6                                       | workflow 扇出(32) + Agent Teams + worktree 隔离                       | QuickJS 沙箱，无 resume                                                 | 8 态状态机完整，record_turn **未接线主循环**                                                        | **MRM**(角色路由/降级链/并发/RPM/账号轮转) | 4 层 loop 检测 + 级联取消 + hook 门控                                                         |
| Claude Code | frontmatter 极细(model/effort/maxTurns/memory/isolation:worktree)                           | 四层(subagent/agent view/teams/dynamic workflows 上限 1000 并发 16)   | Dynamic workflows 纯 JS，6 原语，**journal 确定性 + session 内 resume** | `/goal` 独立小模型逐轮评估自动续跑；**无统一预算熔断**                                              | 无 harness 级调度；账号 5h 窗口            | **无统一 loop 防护层**；官方记录无限评估循环 + $6000 账单案例                                 |
| Codex       | 原生 subagent V2(可配 model/reasoning/concurrency/角色)                                     | 原生并行 spawn + Agents SDK(经 MCP) + cloud best-of-N                 | 无原生脚本 DSL；Agents SDK 是外部编排                                   | `/goal` thread-scoped **运行时驱动** active/blocked/usageLimited/budgetLimited；blocked 需尝试 3 次 | 无 harness 级；account 5h 窗 + 周限额      | goal 状态机含 loop 防护(2026-05/06 修复空耗)                                                  |
| Gemini CLI  | 本地 subagent + A2A 远程 subagent，独立 context                                             | 多 subagent 并行 + A2A；无声明式 workflow 引擎                        | 无 workflow DSL；靠 Plan Mode + Hooks                                   | maxSessionTurns 默认 -1 无限；无独立 goal 状态机                                                    | Auto 路由 + 失败自动降级(模型层)           | Loop Detection Service 默认开(工具+内容双检) + 自动压缩                                       |
| OpenCode    | Agent primary/subagent；内置 Build/Plan/General/Explore/Scout                               | 后台 subagent(实验，需 env flag) + git worktree(社区插件)             | **无原生 workflow 引擎**；markdown 命令 + plugin hooks                  | 无独立 goal 状态机；无硬预算                                                                        | 无 harness 级调度                          | doom_loop 检测(2026-05 跨消息漏检重复 1827 次，截至 2026-07-23 仍 OPEN)；retry 曾无上限致卡死 |
| Crush       | **仅 2 硬编码**(agent/agentic_fetch)，不可自定义                                            | 无 DAG 引擎；多步靠 LLM 递归调 agent 工具                             | **无 workflow 引擎**                                                    | 无独立 goal 状态机                                                                                  | 无资源调度                                 | 工具循环检测(10 步窗口签名>5)，对压缩死循环/变参重试有盲区                                    |
| Cline/Roo   | Cline 三层 + 角色预设(phantom/oracle/anvil/inquisitor via markdown)；Roo Orchestrator+Modes | Cline Teams **任务板+mailbox 持久化** + Kanban 每卡 worktree          | 无脚本 workflow 引擎；靠 Plan/Act + plugin                              | Team task board 跨会话持久；无 goal 预算状态机                                                      | 无 harness 级；hub 负载高时暂停            | `--retries`(默认3) + `--timeout` 硬限；token/费用追踪                                         |
| Goose       | subagent + sub-recipe(可配 provider/model/max_turns)，**均不可再嵌套**                      | worker 池并发**硬顶 10 不可配**；同 MCP extension 并行被 Mutex 串行化 | **无脚本引擎**(维护者明确否决)                                          | 无 goal 预算状态机；max_turns 1000/subagent 25                                                      | **无内置 rate-limit**(功能请求被否)        | RepetitionInspector + `--max-tool-repetitions`；tool 输出截断致无声死循环 #7527               |
| Conductor   | **无角色化 subagent**；靠底层 harness                                                       | 并行单位=workspace(=worktree+分支)                                    | **无脚本引擎**                                                          | 无 goal 状态机；靠 Checkpoints 回滚                                                                 | 无资源调度                                 | **无独立 loop/预算系统**，依赖底层 harness + Checkpoints                                      |
| Vibe Kanban | **无角色化 subagent**                                                                       | workspace+session 多实例 + git worktree                               | **无脚本引擎**                                                          | 无 goal 状态机                                                                                      | 无资源调度                                 | **无 loop 防护/预算/超时熔断**，完全依赖被编排 agent                                          |

来源：https://code.claude.com/docs/en/agents ；https://code.claude.com/docs/en/workflows ；https://developers.openai.com/codex/concepts/subagents ；https://developers.openai.com/codex/use-cases/follow-goals ；https://geminicli.com/docs/core/subagents/ ；https://opencode.ai/docs/agents/ ；https://deepwiki.com/charmbracelet/crush/6.7-agent-tool ；https://docs.cline.bot/features/subagents ；https://docs.cline.bot/cli/agent-teams ；https://github.com/aaif-goose/goose/issues/6615 ；https://github.com/aaif-goose/goose/blob/58f3cc9e/documentation/docs/guides/goose-cli-commands.md (RepetitionInspector) ；https://www.conductor.build/docs/concepts/workflow ；https://vibekanban.com/docs/workspaces/sessions

**差距**：

- P0 goal record_turn 未接线主循环：feature 号称有、运行路径上失效。对标 Codex `/goal` 运行时驱动。
- P1 subagent 角色不可用户自定义：5 预设硬编码于 Rust，max_turns 固定 6，无 per-role model/effort。对标 Claude Code frontmatter、Cline markdown 角色、Codex subagent V2。
- P1 workflow 无 resume/journal：脚本崩溃/取消从头重跑。对标 Claude Code dynamic workflows 的 journal + session 内 resume。
- P1 后台/常驻自治弱：cron 纯内存重启丢失，无持久后台会话。对标 Cline 持久 cron + hub-spoke daemon、Codex cloud。
- P2 Agent Teams 不跨进程存活：`TeamManager::new` 构造时 `remove_dir_all` 清空 teams 目录，team/inbox/task 状态重启清零（`src-tauri/src/agent/team/manager.rs:24-28`）。对标 Cline Teams 任务板/mailbox 持久化跨会话（`docs.cline.bot/cli/agent-teams`）——这是 kxen Agent Teams 相对 Cline 的实质差距。
- P2 GoalUpdate 事件未 publish、agent 活动注册表纯内存不持久、无 A2A/远程 subagent。

**优势**：

- MRM 全局资源调度是 10 家中唯一的 harness 级资源调度器（详见执行摘要 1.3）。
- 4 层递进 loop 检测是最完整的自治可靠性防护（normalize 数字不折叠避免误杀批量遍历，stagnation 结果哈希兜底）。
- QuickJS 沙箱 workflow 是唯二原生脚本编排引擎之一（与 Claude Code Dynamic workflows 同构），硬限制比 Claude Code 更收紧。
- 角色只读权限有单测编译期强约束，取消令牌三检查点级联 + 终态必落库。

### 4.3 维度三：工具执行与安全

**kxen 现状**：无 OS 沙箱，防线为"命令字符串规则 + 路径守卫 + rm 遮蔽"三层软防护；F1-F5 破坏命令语义分类器；auto_bg 15s 转后台；dev server 就绪门；trash 语义删除全链路。

表 1：沙箱与权限审批

| 产品        | OS 级沙箱                                       | 网络隔离                                             | 权限审批档位                                                            | 破坏命令结构化防护                      | 项目信任门                                                                        |
| ----------- | ----------------------------------------------- | ---------------------------------------------------- | ----------------------------------------------------------------------- | --------------------------------------- | --------------------------------------------------------------------------------- |
| kxen        | 无                                              | 无                                                   | 仅 Allow/Deny/Recoverable，无 ask-user                                  | F1-F5 分类器(较强，有解析绕过面)        | **无(自动加载项目级 config/hooks/.agents 无信任确认)**                            |
| Claude Code | 有(Seatbelt/bwrap，仅 Bash 及子进程)            | 有(域名 allowlist，首访提示)                         | default/acceptEdits/auto/dontAsk/bypass/plan/manual 七档 + 规则语法     | rm -rf / 根删除熔断 + auto 分类器       | 有(managed policy 组织级)                                                         |
| Codex       | 有(Seatbelt/bwrap+seccomp/Windows 原生)         | 有(workspace-write 默认关网 + allowlist + 限 method) | sandbox_mode 三档 x approval 三档 + Permission profiles                 | .git/.codex 始终只读 + 无法沙箱化即拒绝 | **有(未 trust 项目跳过 .codex 层含 config/hooks/rules；hooks 按 hash 信任审阅)**  |
| Gemini CLI  | 有(Seatbelt 6 profile/Docker/Podman/gVisor/lxc) | 有(proxied profile + 容器隔离)                       | default/auto_edit/plan/yolo + TOML 策略引擎(allow/deny/ask_user+优先级) | Trusted Folders + 策略引擎              | **有(Trusted Folders：未信任目录禁 hooks/MCP/自动批准/自定义命令)**               |
| OpenCode    | 无(issue #21733 明确缺口，bash 可绕过)          | 无                                                   | allow/ask/deny 三态 + 细粒度 pattern + doom_loop                        | 无结构化破坏防护，仅权限层              | sandbox 内默认不读宿主用户配置                                                    |
| Crush       | 无                                              | 无                                                   | 确认层 + YOLO/super-yolo                                                | 仅 bash 命令硬编码黑名单                | **crush.json 内 `$()` 加载时以当前 shell 权限执行，官方警告不要在未审查目录启动** |
| Cline/Roo   | 无(默认无容器/VM 强隔离)                        | 无                                                   | 工具级审批 + 自动批准分级 + YOLO                                        | 无                                      | 企业远程配置 yoloModeAllowed                                                      |
| Goose       | 无(--container 手动可选)                        | 无                                                   | 4 档(Auto 默认全自主)，无命令级/正则权限(#9407 open)                    | 无                                      | **recipe 首次运行信任弹窗(Operation Pale Fire 后加固)**                           |
| Conductor   | 无(worktree 是开发隔离非安全边界)               | 无                                                   | 透传底层 harness                                                        | 无(依赖底层)                            | 透传底层 harness                                                                  |
| Vibe Kanban | 无(隔离=worktree)                               | 无                                                   | 透传各 CLI flag                                                         | 无                                      | 透传各 CLI                                                                        |

表 2：shell / auto_bg / dev server 就绪门 / 删除保护

| 产品        | shell 执行模型                                                     | 后台任务/auto_bg                                                               | dev server 就绪门                                         | 删除保护                                          |
| ----------- | ------------------------------------------------------------------ | ------------------------------------------------------------------------------ | --------------------------------------------------------- | ------------------------------------------------- |
| kxen        | 每命令全新进程 + 快照回放 + rm->trash 遮蔽(无状态并发安全)         | auto_bg 15s 自动前台转后台；单进程 kill(杀不净进程树)                          | 有：关键词+端口探测+健康检查+restart                      | trash 删除+F1-F5+guard_path+.kxen-bak(较强，多层) |
| Claude Code | 沙箱化 Bash，escape hatch 可退沙箱                                 | run_in_background 显式 flag + BashOutput/KillShell/`/bashes`；headless 5s 宽限 | UNKNOWN(未见专项)                                         | rm -rf / 根删除熔断；无 trash 语义                |
| Codex       | 沙箱化命令，.git 只读，bounded output                              | Codex cloud 两阶段(setup 有网/agent 无网)；本地 best-of-N                      | UNKNOWN                                                   | .git/.codex 只读，无 trash 语义                   |
| Gemini CLI  | run_shell_command + is_background/`&`；持久后台 PTY + 交互式 shell | /shells 面板 + Ctrl+B 转后台 + ShellExecutionService                           | UNKNOWN                                                   | 沙箱写限项目目录，无结构化删除分类                |
| OpenCode    | host 用户完整权限(V2 明确非隔离)                                   | 实验性后台 subagent(env flag) + task_status                                    | UNKNOWN                                                   | .env 默认 deny read；无 trash 语义                |
| Crush       | 用户 shell 直执 + 命令黑名单                                       | bash 内置后台 job(>1min 自动转后台，50 并发)                                   | UNKNOWN                                                   | 无结构化删除保护                                  |
| Cline/Roo   | 工具级执行 + 审批                                                  | Zen Mode 后台 daemon + cron 定时 agent                                         | UNKNOWN                                                   | shadow git checkpoints 可回退，非删除拦截         |
| Goose       | 直接用 $SHELL，无隔离                                              | 内置 cron + headless run；max_turns 默认 1000                                  | UNKNOWN                                                   | 无(权限颗粒度只到整个工具)                        |
| Conductor   | 透传底层 harness(用户全部本地权限)                                 | Cloud workspace(Vercel Sandbox)后台长任务                                      | UNKNOWN                                                   | checkpoints 回退 + worktree，非删除拦截           |
| Vibe Kanban | 透传各 CLI                                                         | PrMonitorService 60s 轮询；无执行时长上限(#1749)                               | 有(Preview Mode Dev Server 日志实时流自动探测 URL 并加载) | 无(依赖底层 CLI)                                  |

来源：https://code.claude.com/docs/en/sandboxing ；https://code.claude.com/docs/en/permissions ；https://developers.openai.com/codex/permissions (未 trust 项目跳过 .codex 层) ；https://developers.openai.com/codex/hooks (hooks 按 hash 信任) ；https://geminicli.com/docs/cli/trusted-folders/ ；https://github.com/anomalyco/opencode/issues/21733 ；https://charmbracelet-crush.mintlify.app/configuration/mcp ($() 警告) ；https://github.com/aaif-goose/goose/issues/9407 ；https://deepwiki.com/block/goose/3.1.4-session-and-recipe-management-ui (recipe 信任弹窗) ；https://vibekanban.com/docs/browser-testing (Dev Server URL 自动探测)

**差距**：

- P0 无 OS 级沙箱：Claude Code/Codex/Gemini 三家均有 Seatbelt/bwrap 级文件+网络隔离，kxen 只有可绕过的命令字符串规则，是安全模型的根本代差。
- P0 无 approval/ask-user 审批档：9 个竞品全有交互审批（哪怕最弱也有确认层），kxen 只能自动放行或硬拒绝，force push/reset --hard/sudo 静默放行。
- P1 无项目信任/配置安全门：kxen 自动加载项目级 `.kxen/config.toml` 的 hooks 与 `.agents/` 知识树（`src-tauri/src/core/config.rs:118-130` merge hooks extend），无任何信任确认；hooks 固定 `/bin/zsh -c` 执行外部命令（`src-tauri/src/tools/hooks.rs`）。恶意仓库可经项目级 hook 在 `pre_tool_use` 时执行任意 zsh 命令。对标 Codex（未 trust 跳过项目级 config/hooks/rules）、Gemini（Trusted Folders）、Goose（recipe 信任弹窗）、Crush（`$()` 警告）。
- P1 无网络隔离/域名 allowlist（Codex/Claude Code/Gemini 有）。
- P1 命令解析绕过面（`||`/换行/反引号/`$()` 不评估），在无沙箱兜底下等于安全洞。
- P1 无进程组/进程树 kill，孙进程泄漏；P1 无用户可配置 allow 规则语法。
- P2 输出截断无落盘兜底；Fish shell 快照缺失；后台任务纯内存不持久。

**优势**：

- auto_bg 15s 自动前台转后台：无需模型显式声明 background，短命令走前台、长命令自动降级。Claude Code/Codex/Gemini 均需显式 background flag，Crush 是 >1min 才转。
- rm->trash 透明遮蔽 + trash 语义删除全链路：竞品普遍无 trash 恢复语义（Codex 靠沙箱只读、Claude Code 靠根删除熔断）。
- F1-F5 破坏命令语义分类器：nested `bash -c` 递归解包 + sudo 前缀剥离 + 变量未求值检测，是独立于沙箱的语义防护层，多数无沙箱竞品（Crush/Goose/OpenCode）只有黑名单或纯权限审批。
- hashline 锚点编辑（chunk fingerprint + find_shifted 自愈 + 免强制 read-before-edit）：竞品资料未见等价机制。
- dev server 就绪门：关键词 + TcpStream 端口探测 + URL 正则解析 + 30s 健康检查 + restart 端口释放等待。这一能力**少见但非独有**——Vibe Kanban Preview Mode 的 Dev Server 日志流会自动探测输出中的 URL 并加载（`vibekanban.com/docs/browser-testing`）；kxen 的差异点在于"健康检查 + restart"一体化，就绪判定本身不再作为独有能力主张。

> 综合定性：kxen 语义分类器是"无沙箱下的最优努力"，但由于缺 OS 沙箱 + ask-user 审批 + 项目信任门（三个 P0/P1），kxen 整体安全模型仍弱于 Claude Code/Codex/Gemini 三家。

### 4.4 维度四：会话与 UI 体验

**kxen 现状**：单一原生 macOS GUI（Tauri，仅 Apple Silicon），无 CLI/TUI/IDE/Web 形态；fork 分支 + 单文件 undo，无 checkpoint/rewind；worktree dock 面板，无并行看板；composer 一体化程度高；语音 PTT 双引擎；应用内通知无 OS 推送。

表 1：形态 + 会话分支 + worktree 并行

| 产品        | 产品形态                                               | 会话分支/checkpoint                                                          | worktree 并行                                                             |
| ----------- | ------------------------------------------------------ | ---------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| kxen        | 原生 Tauri macOS GUI(仅 Apple Silicon)，无 CLI/IDE     | fork + 编辑重发 fork + .kxen-bak 单文件 undo；无 checkpoint/rewind           | dock 面板 create/remove + 子代理 worktree 隔离；无并行 workspace 看板     |
| Claude Code | 终端 CLI + VS Code/JetBrains + Desktop + Web + Cloud   | checkpointing(最近 100 快照) + /rewind 五档 + /branch + --fork-session       | agent view 自动每会话独立 worktree + subagent isolation:worktree + /batch |
| Codex       | Rust CLI(TUI) + IDE 扩展 + ChatGPT 桌面 + cloud        | /resume + /fork + /side /btw 侧聊 + contextual branch；分页历史              | 原生 codex worktree CLI + 桌面 worktree 面板 + Handoff                    |
| Gemini CLI  | 纯终端 CLI(IDE 靠 Companion/ACP)                       | 自动保存 + /resume 浏览器 + tag checkpoint + /rewind + 影子 git              | --worktree 实验(v0.36)，不自动并行编排，需手动多终端                      |
| OpenCode    | TUI + 桌面 App(Beta) + VS Code/Zed，client/server      | /sessions resume + --fork + /undo /redo(依赖 git) + /share 公开链接          | worktree 仅设计草案未合并，靠社区插件                                     |
| Crush       | 终端 TUI，无 GUI                                       | session list/show/rename/delete；无 branch/fork/checkpoint/undo              | 无原生，靠第三方 cwt/crush-sandbox                                        |
| Cline/Roo   | Cline VS Code+CLI+SDK+Kanban+菜单栏；Roo VS Code(停运) | checkpoints(shadow git 3 档恢复) + history + --continue；Kanban 卡片         | Cline 领先：CLI --worktree 自动 + Kanban 每卡片独立 worktree              |
| Goose       | Electron 桌面 + CLI + REST server                      | resume + Duplicate + Fork(从消息截断) + --history 回放                       | 无原生自动化(团队明确拒绝)，仅 worktree 切换器 RFC                        |
| Conductor   | 原生 macOS App(封装他人 harness)                       | checkpoints(私有 git ref 时间旅行) + fork 到 workspace/tab + archive/restore | 核心单元=workspace=worktree+分支(GUI worktree 最强)                       |
| Vibe Kanban | 本地 Web 服务 + 后补 Tauri 壳                          | workspace+session；编辑历史重试；无 checkpoint/对话分支                      | 每 task attempt 独立 worktree，多 workspace 并行(核心模型)                |

表 2：composer + 命令面板 + 通知 + 语音

| 产品        | composer                                                                          | 命令面板                               | 通知                                                      | 语音                                                                                |
| ----------- | --------------------------------------------------------------------------------- | -------------------------------------- | --------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| kxen        | 原生 textarea + @//#触发 + 每会话草稿 + 粘贴占位 + IME 处理 + 内嵌模型/角色选择器 | Cmd/Ctrl+K 三路(command/session/model) | 应用内铃铛+未读角标 5s 轮询；无 OS 推送，后台会话事件丢弃 | 双引擎 PTT(Apple 本地流式 + 云转写 + Wispr 升级)，空格长按                          |
| Claude Code | 终端 composer + @-mention + plan review                                           | slash + /plugin /hooks /workflows TUI  | 终端通知 hook + 桌面 app                                  | 有：`/voice` hold/tap 双模式 PTT，CLI 与桌面 App 均支持；纯云端转写非本地识别       |
| Codex       | TUI composer(可配 keymap/statusline/主题) + 历史                                  | slash + /status /usage                 | 菜单栏/桌面                                               | 部分：ChatGPT 桌面 App 内 hold Ctrl+M 云端转写；Codex CLI/TUI 自身语音 2026-03 移除 |
| Gemini CLI  | 终端 + @ mention + todos 进度条                                                   | 丰富 slash 命令集                      | Notification hook 事件                                    | UNKNOWN(comp 未提及)                                                                |
| OpenCode    | TUI @path 模糊 + !command shell 直通；桌面 tabs                                   | 17 内置 slash + 命令面板               | toast + 桌面 App                                          | UNKNOWN(comp 未提及)                                                                |
| Crush       | 多行 textarea + 附件托盘 + question tool 表单                                     | ctrl+l 模型选择器 + 栈式 dialog        | 桌面通知(OSC 99/777/beeep)，失焦触发                      | UNKNOWN(comp 未提及)                                                                |
| Cline/Roo   | OpenTUI 或 VS Code；@file/@folder/@url/@problems                                  | slash + Mode 下拉                      | 菜单栏 App 系统通知 + 聊天连接器(Telegram/Slack/Discord)  | UNKNOWN                                                                             |
| Goose       | ChatInput 文本/图片/语音听写/模型切换                                             | Cmd+F 搜索                             | 桌面 App 通知                                             | 有(桌面语音听写，纯云端，能力弱于 kxen)                                             |
| Conductor   | 富文本 @-mention + 消息队列 + 粘贴附件 + 上下文用量条 + Cmd+Z(GUI 最强)           | Cmd+K + Dispatcher + Passport          | 桌面(机制 UNKNOWN)                                        | UNKNOWN                                                                             |
| Vibe Kanban | composer + inline diff 评论回传 agent                                             | Cmd/Ctrl+K + vim 前缀键                | Tauri 原生通知深链；webhook 为提案                        | UNKNOWN                                                                             |

来源：https://code.claude.com/docs/en/checkpointing ；https://code.claude.com/docs/en/voice-dictation.md (Claude Code /voice 纯云端) ；https://developers.openai.com/codex/app/worktrees ；https://github.com/openai/codex/pull/16114 (Codex CLI 语音 2026-03 移除) ；https://geminicli.com/docs/cli/git-worktrees/ ；https://opencode.ai/docs/share/ ；https://cline.bot/cli ；https://www.conductor.build/docs/reference/checkpoints ；https://github.com/BloopAI/vibe-kanban/pull/2896

> 语音列口径统一（修正原不一致）：仅当竞品调研文件明确记载"有/无"时才如此标注，未覆盖处一律 UNKNOWN。据此 Gemini CLI/OpenCode/Crush 的 comp 文件均未提及语音，改标 UNKNOWN（原 dim-session-ui 表将其标"无"证据不足）；Cline/Conductor/Vibe Kanban 本就 UNKNOWN。Claude Code/Codex 已由补充调研核实为"有"（`extra-1.md`）。

**差距**：

- P1 无 checkpoint/rewind 时间旅行：仅 fork + 单文件 .kxen-bak + 内存 diff 快照(不可 revert)。对标 Claude Code(100 快照+/rewind)、Gemini(影子 git+/rewind)、Cline/Roo(shadow git 3 档)、Conductor(私有 git ref 时间旅行)。
- P1 后台/多会话无 OS 推送与可见性：`delta.ts` 丢弃非活跃会话事件，长后台任务与 Agent Teams 跑完在别的会话无信号。对标 Vibe Kanban(Tauri 原生通知深链)、Cline(菜单栏系统通知)、Crush(桌面通知)。
- P1 无并行 workspace 看板中心视图。对标 Conductor(workspace=worktree 核心)、Vibe Kanban(看板并行)、Cline Kanban(每卡片 worktree)。
- P2 composer 非图片附件仅文件名 chip、web/docs chip 不可达、无拖拽上传；无 diff viewer 行内评论回传闭环；SessionTree 单组超 5 条不可展开；无会话公开分享链接；无 IDE 集成面(定位取舍)；无 UI 内 PR/GitHub 流程。

**优势**：

- 真正的原生 GUI 优先设计（Tauri）+ 自研 agent：CLI/TUI 为主的 5 家无原生 GUI；Goose 是 Electron、Vibe Kanban 是 Web 服务后补 Tauri 壳、Conductor 虽原生但封装他人 harness。kxen 是少数"自研 agent + 原生 GUI 一体"。
- composer 一体化程度高：@//#三触发 + 每会话草稿 + 大粘贴占位撤销 + WebKit IME 锁窗 + 内嵌模型选择器与"分配为角色"，把角色路由配置直接放进输入区。
- 语音收窄后的独有点：kxen 是 10 家里唯一做"Apple Speech.framework 本地流式识别（离线可用、无服务器往返）+ 云转写降级 + Wispr 三层混合"的（`src-tauri/src/voice/mod.rs:104-134`）；Claude Code/Codex 的听写均为纯云端转写、无本地识别环节。**PTT 交互形态本身已非独有**（Claude Code `/voice`、Codex hold Ctrl+M 均成体系）。
- 命令面板三路混合搜索单入口；右栏一体化（子代理编排 FocusView + goal/改动 diff/后台任务 Dock + worktree 面板）。

### 4.5 维度五：知识与记忆

**kxen 现状**：OKF 单规范统一 7 类知识 + 双 scope；注入四态分级引擎级；globs 条件激活 + mid-turn 刷新；会话删除兜底蒸馏；仅认 AGENTS.md；记忆全文注入无检索。

| 产品        | 静态规则                    | 第三方格式互操作                                      | scope 分层                         | 注入机制                                   | 条件激活                   | 自写记忆                                       | 记忆检索                         | 渐进披露 skill                           |
| ----------- | --------------------------- | ----------------------------------------------------- | ---------------------------------- | ------------------------------------------ | -------------------------- | ---------------------------------------------- | -------------------------------- | ---------------------------------------- |
| kxen        | .agents/rules(OKF 7 类统一) | 仅 AGENTS.md                                          | project + personal                 | 引擎级四态分级                             | globs + mid-turn 刷新      | knowledge 5 类 note + 删除蒸馏                 | 无(全文注入截断 500)             | 有(discovery/load+needs 懒加载+递归防护) |
| Claude Code | CLAUDE.md + .claude/rules/  | /init 整合 cursor/copilot/AGENTS/.clinerules          | user+project+local+managed(组织级) | 目录树向上全量拼接 + @import；压缩后重注入 | .claude/rules/ 路径范围    | Auto memory(默认开，MEMORY.md 索引)            | 索引式(MEMORY.md 索引，主题按需) | 有(name+desc)                            |
| Codex       | AGENTS.md(override+逐级)    | 有限(AGENTS.md 家族)                                  | 全局 override + 项目逐级           | 根到 cwd 拼接，32KiB 截断                  | 无(静态全量)               | Memories 两阶段后台 pipeline(SQLite+redaction) | 有(usage_count/last_usage 排序)  | 有(Skills/Plugins)                       |
| Gemini CLI  | GEMINI.md 三层              | 可自定义文件名(含 AGENTS.md)                          | 全局+工作区+JIT                    | 三层拼接 + JIT + @import                   | JIT 目录级                 | Memory 工具(细节 UNKNOWN)                      | UNKNOWN                          | 有(四阶段)                               |
| OpenCode    | AGENTS.md                   | CLAUDE.md V1 兜底，V2 移除                            | 项目+全局                          | 合并注入；instructions V2 未生效           | lazy loading(提示词层约定) | 靠社区插件 opencode-agent-memory               | 插件层块检索                     | 有                                       |
| Crush       | CRUSH.md + AGENTS.md        | 全兼容合并(copilot/.cursorrules/CLAUDE/GEMINI/AGENTS) | 项目+全局                          | 全部读取合并注入                           | 无(全量)                   | 无                                             | 无                               | 有(agentskills.io)                       |
| Cline/Roo   | .clinerules/ / .roorules    | 自动识别 .cursorrules/.windsurfrules/AGENTS.md        | 工作区+全局                        | 合并注入 + hook 动态注入                   | 条件规则 paths frontmatter | Memory Bank(方法论，非内置)                    | 无内置                           | Cline 有 subagent 预载 skills            |
| Goose       | .goosehints(兼容 AGENTS.md) | AGENTS.md(可配文件名)                                 | 全局+项目                          | 每次整份注入 + MOIM 每轮                   | 无(全量)                   | Memory Extension(MCP，tag 存储)                | 有(关键词/tag 检索)              | recipe 化 slash command                  |
| Conductor   | 无自有(透传底层)            | 透传                                                  | 依赖底层                           | 依赖底层                                   | 依赖底层                   | .context 目录(非结构化)                        | 无                               | 依赖底层                                 |
| Vibe Kanban | 无自有                      | 不解析，透传                                          | 无                                 | 无项目级注入(痛点 #1979)                   | 无                         | 无(append_prompt workaround)                   | 无                               | 依赖底层                                 |

来源：https://code.claude.com/docs/en/memory ；https://developers.openai.com/codex/memories ；https://geminicli.com/docs/cli/gemini-md/ ；https://opencode.ai/docs/rules/ ；https://charmbracelet-crush.mintlify.app/guides/context-files ；https://docs.cline.bot/customization/cline-rules ；https://goose-docs.ai/docs/guides/context-engineering/using-goosehints/ ；https://github.com/BloopAI/vibe-kanban/issues/1979

**差距**：

- P1 第三方规则格式互操作缺失：仅认 AGENTS.md，不识别 CLAUDE.md/GEMINI.md/.cursorrules 等。对标 Crush(全兼容合并)、Cline(自动识别)、Claude Code(/init 整合)。从其他工具迁移的用户需手工改名。
- P1 记忆无动态检索，全文注入随规模线性膨胀。对标 Goose Memory Extension(tag 检索)、Codex Memories(usage_count 排序)。
- P1 无后台持续记忆 consolidation：蒸馏仅在删会话时触发且不通知前端。对标 Codex 两阶段后台 pipeline、Claude Code Auto memory(会话中持续写、压缩后重注入)。
- P2 injection_preview 不感知会话文件；无组织/managed policy 强制层(对标 Claude Code managed policy)；rules/reference 无 @import 模块化组合。

**优势**：

- OKF 单规范统一 7 类知识：竞品普遍是多套割裂子系统，kxen 单一 knowledge 模块。
- 注入四态分级是引擎级行为而非提示词约定：对比 OpenCode 的 lazy loading 需手写指令教模型看到 @ 就 read（`render.rs:47-49` vs `comp-opencode` 第64行）。
- glob 条件激活 + mid-turn 刷新联动：本轮文件集变化即重建 system prompt 重新激活，粒度比 Cline/Claude Code 的 paths 路径范围更细。
- 多层就近 AGENTS.md 动态就近（Codex/Claude Code 是启动时静态全量拼接）。
- 会话删除兜底蒸馏是独特触发时机；结构化 note 类型 + 幂等覆盖；显式 scope 晋升/降级(move_entry)；skill 加载工程化防护(递归 cap 3 + 同参去重 + disable_model_invocation)。

### 4.6 维度六：可靠性与防失控

**kxen 现状**：4 层 loop 检测（运行时强制层，唯一扎实防护）；三维预算 + 阻塞升级设计完整但 record_turn 未接线；无 checkpoint/回滚（仅内存 diff 快照不可 revert + 对话 fork）；doctor 仅凭据一维且文案与实现不符；无 LLM 请求重试。

| 产品        | Loop 检测                                                                 | 预算控制                                                              | 阻塞升级                          | checkpoint/回滚                                                                                                                                                                    | doctor 自检                                                                                                                                          |
| ----------- | ------------------------------------------------------------------------- | --------------------------------------------------------------------- | --------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| kxen        | 四层(exact/semantic/stagnation/churn)，真实接入 4 类运行路径，运行时强制  | 三维预算设计完整但**未接线 agent_loop**                               | 三次规则完整但同未接线            | 无 shadow-git；仅内存 diff 快照(不可 revert)+对话 fork                                                                                                                             | 仅凭据维度，文案与实现不符                                                                                                                           |
| Claude Code | **无统一 loop 防护层**，曾有无限评估循环 bug                              | 无硬预算，曾 $6000 定时任务账单；/goal 靠用户自写 "stop after N"      | /goal 软约束，无强制升级          | checkpoint(100 快照)/worktree(worktree 可见性 bug)                                                                                                                                 | 覆盖 9 类以上(安装/更新/settings 合法性/权限规则遮蔽/MCP 配置与连接健康/keybinding/插件与 agent 加载/上下文体积告警/Keychain/Remote Control/ripgrep) |
| Codex       | loop 防护 2026-05/06 两次修复防限额/终态无限重试                          | /goal budgetLimited/usageLimited **已接线运行时**                     | /goal 状态机含 blocked **已接线** | 对话层 Esc-Esc/thread.rollback 只回退对话(已 deprecated)；代码层 /undo 已移除；对话+代码一体 checkpoint(#11626/#12558)截至 2026-07-23 仍 OPEN；桌面 App 仅 worktree 删除前整体快照 | UNKNOWN                                                                                                                                              |
| Gemini CLI  | Loop Detection Service 默认开(工具+内容双检)                              | maxSessionTurns 默认 -1；曾改 15 当天 revert；有自动压缩              | UNKNOWN                           | Checkpointing 影子 git(默认关) + /rewind                                                                                                                                           | UNKNOWN                                                                                                                                              |
| OpenCode    | doom_loop 检测，2026-05 跨消息漏检重复 1827 次，截至 2026-07-23 未修复    | 无硬预算上限；compaction.auto 默认开                                  | UNKNOWN                           | /undo-redo 依赖 git(非真 redo 栈)，无 checkpoint                                                                                                                                   | UNKNOWN                                                                                                                                              |
| Crush       | 工具循环检测(10 步窗口签名>5)，对压缩死循环/变参重试有盲区                | 无内置预算；有自动摘要(死循环 bug #2551)                              | UNKNOWN                           | 无 branch/checkpoint/undo                                                                                                                                                          | UNKNOWN                                                                                                                                              |
| Cline/Roo   | 防护 UNKNOWN；Cline --retries(默认3)                                      | 无硬预算；Cline token/费用追踪                                        | UNKNOWN                           | Checkpoints(shadow git)；Roo 嵌套 git 静默禁用 bug                                                                                                                                 | UNKNOWN                                                                                                                                              |
| Goose       | RepetitionInspector + --max-tool-repetitions；tool 截断致无声死循环 #7527 | max_turns 即唯一粗预算，无 token/时钟预算；有 tool pair summarization | UNKNOWN                           | 无原生 checkpoint                                                                                                                                                                  | UNKNOWN                                                                                                                                              |
| Conductor   | 无独立 loop 防护，依赖底层 harness                                        | 无独立预算，依赖底层额度                                              | 依赖底层                          | Checkpoints 每轮前写私有 git ref，一键回退文件/git/聊天                                                                                                                            | 无(依赖底层)                                                                                                                                         |
| Vibe Kanban | 无 loop 防护                                                              | 无 token/费用预算、无超时熔断                                         | 无                                | 无 checkpoint/回滚                                                                                                                                                                 | 无                                                                                                                                                   |

来源：https://github.com/anthropics/claude-code/issues/68911 (无限评估循环) ；https://developers.openai.com/codex/use-cases/follow-goals ；https://github.com/openai/codex/issues/11626 与 #12558 (checkpoint OPEN) ；https://github.com/google-gemini/gemini-cli/blob/caa04664/packages/core/src/services/loopDetectionService.ts ；https://github.com/anomalyco/opencode/issues/25254 ；https://github.com/aaif-goose/goose/issues/7527 与 goose-cli-commands.md (RepetitionInspector) ；https://www.conductor.build/docs/reference/checkpoints ；https://code.claude.com/docs/en/debug-your-config (claude doctor 覆盖范围)

**差距**：

- P0 预算控制未接线 agent_loop（record_turn 仅 RPC 可达），运行中预算不生效。对标 Codex `/goal` 运行时驱动。
- P0 阻塞三次升级未接线运行路径，运行中不自动升级。对标 Codex。
- P0/P1 无上下文压缩（compaction）：全库 grep 零命中，长会话必然撞上下文上限无缓解手段。对标 Claude Code/Gemini/OpenCode/Codex/Goose/Crush（详见执行摘要 1.2 第 3 条）。
- P1 无 shadow-git per-turn checkpoint 与一键回退；内存 diff 快照不可 revert 且重启丢失。对标 Gemini/Cline/Conductor。
- P1 LLM 请求无重试/backoff/换账号韧性。对标 OpenCode。
- P1 doctor 仅覆盖凭据一维：claude doctor 官方文档确认覆盖安装完整性/自动更新/settings 合法性/权限规则遮蔽/MCP 配置与连接健康/keybinding/插件与 agent 加载/上下文体积告警/Keychain/Remote Control 共 9 类以上，对标强度成立且差距面比原表述更广（"网络连通性/模型可达"两项官方文档未证实，不作坐实点）。
- P2 doctor 文案 "will refresh on next call" 与无 refresh 实现不符；GoalUpdate 从不 publish；4 类 loop detector 各自独立，跨 agent 协同循环无法检测。

**优势**：

- 四层递进 loop 检测覆盖面强于多数竞品：Crush 单一签名窗口有盲区、OpenCode 曾跨消息漏检 1827 次（时效性核实见 7.3）、Goose 有无声死循环 bug，kxen 额外覆盖 churn（编辑/回滚振荡）与 stagnation（结果不变）且真实接入 4 类运行路径。normalize 数字不折叠避免误杀合法批量遍历。
- Goal 状态机形式化最完整：8 态 + 显式迁移表 + 非法迁移报错 + complete 强制 evidence 非空 + 三维预算 + 阻塞三次规则（设计完整度超 Codex，唯一短板是未接线）。
- workflow 编排层硬性资源上限（10min/32 agent/64MB/1MB 栈 + CancelGuard Drop）比 Claude Code dynamic workflows(1000 agent/并发 16)更保守。
- MRM 资源可观测层（并发 semaphore + RPM 滑窗 + 派发历史 + describe() 诊断）；CancelToken 三检查点级联；Keychain 探测 5s 超时；auth.json 0600 + tmp+rename 原子写。

### 4.7 维度七：生态与扩展性

**kxen 现状**：无 MCP、无 LSP、无插件市场、无 SDK、无 headless/CI；hooks partial（2 事件 + 2 team 事件，单 zsh handler）；skills 较强（OKF）；非开源仅单平台；BYO 订阅零计费。

| 产品        | MCP                                | LSP/代码智能                                             | Hooks                                       | 插件市场                                    | Skills                    | SDK                                 | headless/CI                                          | License             | Pricing                          |
| ----------- | ---------------------------------- | -------------------------------------------------------- | ------------------------------------------- | ------------------------------------------- | ------------------------- | ----------------------------------- | ---------------------------------------------------- | ------------------- | -------------------------------- |
| kxen        | 无                                 | 无(design 6.6 planned，grep 零命中)                      | partial(2+2 事件，单 handler)               | 无                                          | 有(OKF 双 scope)          | 无                                  | 无                                                   | 无 LICENSE(private) | BYO 订阅探测零计费，多账号池化   |
| Claude Code | 全(stdio/SSE/HTTP/WS+tool search)  | 插件可打包 LSP server                                    | 强(30 事件，四类 handler)                   | 强(官方+社区+自建，5 种安装)                | 有(/batch、subagent 预载) | 有(TS 非 MIT / Python MIT)          | 有(claude -p、--bare、GitHub Action)                 | CLI 闭源            | Pro/Max/Team/Enterprise 捆绑订阅 |
| Codex       | 有(codex mcp-server + Agents SDK)  | UNKNOWN                                                  | 有(对齐 Claude 命名，按 hash trust)         | 有(.codex-plugin/plugin.json + marketplace) | 有                        | 有(TS JSONL / Python beta JSON-RPC) | 有(codex exec)                                       | Apache 2.0          | ChatGPT 订阅 / API key           |
| Gemini CLI  | 有(3 种传输)                       | UNKNOWN                                                  | 有(11 事件，同步执行)                       | 有(100+ 官方扩展市场)                       | 有(渐进披露)              | 未发布(设计草案)                    | 有(官方 GitHub Action)                               | Apache 2.0          | Google 生态                      |
| OpenCode    | 成熟(本地/远程+自动 OAuth)         | **内置约 40 种语言 LSP server(诊断喂 agent)**            | 丰富(plugin hook)                           | 有(Plugin + hook 体系)                      | 部分(markdown 命令)       | 有(官方 @opencode-ai/sdk)           | 有(opencode serve + GitHub Action)                   | MIT                 | 软件免费，Zen/Go/Black 网关      |
| Crush       | 有(stdio/http/sse)                 | **有(lsp_diagnostics/lsp_references/lsp_rename 等工具)** | 弱(仅 PreToolUse，兼容 Claude Code hooks)   | UNKNOWN(有 client-server API)               | 有(agentskills.io)        | 部分(REST API)                      | 部分(crush server/run)                               | FSL-1.1-MIT         | 免费，Hyper 订阅可选             |
| Cline/Roo   | 有(Cline host/client)              | UNKNOWN                                                  | 有(Cline 8 文件 hook + 类型化 runtime hook) | 有(Cline 完整 Plugin 扩展点)                | 有(自定义 mode/角色预设)  | 有(Cline @cline/core SDK)           | 有(cline --json、--zen daemon、cline schedule)       | Cline 开源          | ClinePass $9.99/mo / BYOK        |
| Goose       | 有(扩展生态完全基于 MCP)           | UNKNOWN                                                  | 无 hooks 系统                               | 无插件(扩展=MCP only)                       | 有(recipe/sub-recipe)     | 无独立 Agent SDK                    | 有(goose run headless + goose serve ACP + 内置 cron) | Apache 2.0          | 免费，BYO provider               |
| Conductor   | 透传底层 harness                   | 透传                                                     | 透传                                        | 透传                                        | 透传                      | 无公开(内部 workspace API)          | 无公开                                               | 闭源                | 免费(BYO 订阅)                   |
| Vibe Kanban | 双向 MCP(配外部 + 暴露本地 server) | 透传                                                     | 无(转发各 CLI)                              | 无                                          | 无(透传 CLI)              | 无(headless token #827 未合并)      | UNKNOWN(feature request 未合并)                      | Apache 2.0          | 本地免费；Cloud 已关停           |

来源：https://code.claude.com/docs/en/mcp ；https://code.claude.com/docs/en/plugins (可打包 LSP server) ；https://developers.openai.com/codex/plugins ；https://github.com/openai/codex/blob/HEAD/LICENSE (Apache 2.0) ；https://geminicli.com/docs/tools/mcp-server/ ；https://opencode.ai/docs/lsp/ (约 40 种 LSP server) ；https://github.com/charmbracelet/crush/blob/main/docs/hooks/README.md ；README (Crush lsp_diagnostics/lsp_references/lsp_rename) ；https://docs.cline.bot/sdk/plugins ；https://github.com/aaif-goose/goose (扩展=MCP) ；https://vibekanban.com/docs/integrations/vibe-kanban-mcp-server

**差距**：

- P0 MCP 完全缺失：9 个竞品全部支持 MCP，kxen 唯一零 MCP 通道者，切断第三方工具/数据源接入的行业标准通道。
- P1 LSP/代码智能缺失：全库 grep `lsp/Lsp/LSP` 零命中，design 6.6 planned。对标 OpenCode(内置约 40 种语言 LSP server，`opencode.ai/docs/lsp/`)、Crush(lsp_diagnostics/lsp_references/lsp_rename 工具)、Claude Code(插件可打包 LSP server)。研究/重构型任务缺代码智能反馈信号。
- P1 无 SDK/编程接口：无法被外部程序驱动或二次开发。对标 Claude Code(TS+Python)、Codex(TS+Python)、OpenCode、Cline。
- P1 无 headless/CI 模式：GUI-only，不能进 CI/无人值守流水线。对标 Claude Code(claude -p + Action)、OpenCode(serve)、Goose(run/serve)、Gemini(Action)。
- P1 无插件/marketplace 系统。对标 Claude Code(三市场)、Gemini(100+ 扩展)、Codex、Cline。
- P1 web 检索能力缺失：仅 webfetch(正则剥 HTML，deferred)，design 3.2 承诺的 websearch 未实现(`scan-docs-intent` 第16节)。对标 OpenCode(websearch/codesearch 权限维度)、Crush(agentic_fetch 深度研究子代理 + sourcegraph)、Gemini(搜索 grounding 生态)。研究型任务的信息获取面是实际差距。
- P2 hooks 事件与 handler 覆盖不足(缺 notification/stop/session_start，handler 仅 zsh)；非开源/无 LICENSE；跨平台缺失(仅 aarch64-apple-darwin)。

**优势**：

- OKF 统一知识系统结构化程度领先(详见 4.5)。
- BYO 多订阅并存 + 多账号池化最彻底(详见 4.1)。
- 自定义 provider 双协议(openai/anthropic) + 端点模型清单，覆盖长尾自建网关。

---

## 5. 差距总清单（按 P0/P1/P2 排序）

### P0（生产阻断级 / 核心能力缺失）

| #    | 差距                                                   | kxen 证据                                                   | 对标产品                                                                          |
| ---- | ------------------------------------------------------ | ----------------------------------------------------------- | --------------------------------------------------------------------------------- |
| P0-1 | 无 MCP(第三方工具/数据源接入标准通道)                  | grep 零命中；design 6.6 planned                             | Claude Code / Codex / OpenCode / Goose(全部 9 家)                                 |
| P0-2 | 无请求重试/退避/换账号                                 | `agent_loop/run.rs:115-119`，llm 层 grep 零命中             | Claude Code(RETRY_WATCHDOG) / OpenCode(RETRY_MAX_ATTEMPTS) / Goose(retry_manager) |
| P0-3 | 无 OAuth token 主动刷新(委托官方 CLI，doctor 文案不符) | `auth/probe.rs:57-60`；`doctor.rs:29,42`                    | Codex / Gemini / Cline / Crush(30s 自动刷新)                                      |
| P0-4 | 无上下文压缩(compaction)，长会话必撞上限无缓解         | grep `compact` 零命中；ctx 硬编码 200k `ws/settings.rs:213` | Claude Code / Gemini / OpenCode / Codex / Goose / Crush                           |
| P0-5 | 无 OS 级沙箱(文件+网络隔离)                            | tools/safety 无 Seatbelt/bwrap/容器调用                     | Claude Code / Codex / Gemini                                                      |
| P0-6 | 无 approval/ask-user 交互审批档                        | `safety/rules.rs:8` Verdict 仅 3 变体                       | 全部 9 家(均有确认层)                                                             |
| P0-7 | goal 预算/阻塞升级未接线主循环，自治闭环断裂           | record_turn 仅 `goal_rpc.rs:66` 可达                        | Codex `/goal`(运行时驱动)                                                         |

### P1（能力代差 / 采用门槛）

| #     | 差距                                                | kxen 证据                                                        | 对标产品                                                                                  |
| ----- | --------------------------------------------------- | ---------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| P1-1  | 无项目信任/配置安全门(恶意仓库 hook 可执行任意 zsh) | `config.rs:118-130` 自动 merge 项目级；`tools/hooks.rs` zsh 执行 | Codex(未 trust 跳过) / Gemini(Trusted Folders) / Goose(recipe 信任弹窗) / Crush($() 警告) |
| P1-2  | 无 LSP/代码智能                                     | grep `lsp` 零命中；design 6.6 planned                            | OpenCode(约 40 LSP) / Crush(lsp_* 工具) / Claude Code(插件)                               |
| P1-3  | 无 SDK / 编程接口                                   | `src-tauri/src/bin` 不存在                                       | Claude Code / Codex / OpenCode / Cline                                                    |
| P1-4  | 无 headless / CI 模式                               | main.rs 仅 run/ws_port                                           | Claude Code / OpenCode / Goose / Gemini                                                   |
| P1-5  | 无插件 / marketplace                                | 无插件系统                                                       | Claude Code / Gemini(100+) / Codex / Cline                                                |
| P1-6  | 无 web 检索(websearch 未实现)                       | scan-docs-intent 第16节                                          | OpenCode(websearch/codesearch) / Crush(agentic_fetch+sourcegraph) / Gemini(grounding)     |
| P1-7  | 无 checkpoint/rewind 时间旅行                       | 仅 fork + .kxen-bak + 内存快照不可 revert                        | Claude Code / Gemini / Cline / Conductor                                                  |
| P1-8  | subagent 角色不可用户自定义，max_turns 固定 6       | `subagent.rs:80-90,114` 硬编码                                   | Claude Code(frontmatter) / Cline(markdown) / Codex(V2)                                    |
| P1-9  | workflow 无 resume/journal                          | 无缓存/回放代码                                                  | Claude Code dynamic workflows(journal+resume)                                             |
| P1-10 | 无内置 provider 广度(仅 4 家)                       | `catalog.rs:11-16`                                               | OpenCode(75+) / Crush(30) / Cline(15+) / Goose(15-30+)                                    |
| P1-11 | 运行时错误后不自动降级/换账号                       | MRM 仅选模型阶段生效                                             | Gemini(ModelAvailabilityService)                                                          |
| P1-12 | 记忆无动态检索，全文注入线性膨胀                    | notes 全文注入截断 500                                           | Goose(tag 检索) / Codex(usage_count 排序)                                                 |
| P1-13 | 无后台持续记忆 consolidation                        | 蒸馏仅删会话时触发不通知前端                                     | Codex(两阶段 pipeline) / Claude Code(Auto memory)                                         |
| P1-14 | 第三方规则格式互操作缺失(仅 AGENTS.md)              | `scan.rs` 仅 AGENTS.md 分支                                      | Crush(全兼容) / Cline(自动识别) / Claude Code(/init)                                      |
| P1-15 | 后台/多会话无 OS 推送与可见性                       | `delta.ts:41-42` 丢弃非活跃事件                                  | Vibe Kanban / Cline / Crush                                                               |
| P1-16 | 无并行 workspace 看板中心视图                       | worktree 仅 dock 面板                                            | Conductor / Vibe Kanban / Cline Kanban                                                    |
| P1-17 | doctor 仅覆盖凭据一维                               | `doctor.rs:24-60`                                                | Claude Code(9 类以上)                                                                     |
| P1-18 | 无 shadow-git per-turn checkpoint 与回滚            | 内存快照不可 revert 且重启丢                                     | Gemini / Cline / Conductor                                                                |
| P1-19 | 无网络隔离/域名 allowlist                           | tools/ 无相关调用                                                | Codex / Claude Code / Gemini                                                              |
| P1-20 | 命令解析绕过面(`                                    |                                                                  | `/换行/反引号/`$()`)                                                                      | `safety/eval.rs:39` | (无沙箱兜底下等于安全洞) |

### P2（增量能力 / 定位取舍 / 边界）

| #     | 差距                                                  | kxen 证据                               | 对标产品                                               |
| ----- | ----------------------------------------------------- | --------------------------------------- | ------------------------------------------------------ |
| P2-1  | Agent Teams 不跨进程存活                              | `team/manager.rs:24-28` remove_dir_all  | Cline Teams(任务板/mailbox 持久化)                     |
| P2-2  | 无进程组/进程树 kill，孙进程泄漏                      | `task.rs:92` 单进程 SIGKILL             | Gemini / Crush                                         |
| P2-3  | hooks 事件与 handler 覆盖不足                         | `tools/hooks.rs` 缺 3 通用事件          | Claude Code(30 事件) / Gemini(11)                      |
| P2-4  | 无组织/managed policy 强制层                          | 仅 project+personal 双 scope            | Claude Code managed policy                             |
| P2-5  | GoalUpdate 事件从不 publish                           | `core/event.rs:10`                      | 各家事件驱动 UI                                        |
| P2-6  | agent 活动注册表纯内存不持久                          | `activity.rs`                           | Cline mission log 持久化                               |
| P2-7  | 无 A2A/远程 subagent                                  | 仅本地进程内                            | Gemini CLI A2A                                         |
| P2-8  | 无 rules/reference @import 模块化                     | needs 仅 skill/command                  | Claude Code / Gemini @path import                      |
| P2-9  | composer 附件/web-docs chip/拖拽上传不全              | `TextComposer.tsx:102,117,119`          | Conductor                                              |
| P2-10 | 无 diff viewer 行内评论回传闭环                       | Dock 有 diff 无回传                     | Conductor / Vibe Kanban / Cline                        |
| P2-11 | 无会话公开分享链接 / 无 IDE 集成面 / 无 UI 内 PR 流程 | 定位取舍                                | OpenCode(/share) / 多数竞品 IDE / Conductor(Create PR) |
| P2-12 | 非开源 / 无 LICENSE / 仅单平台                        | package.json private；仅 aarch64-darwin | OpenCode/Gemini/Goose/VK(开源三平台)                   |
| P2-13 | 无成本/token 预算硬停可视化                           | goal budget 未接线                      | (OpenCode/Codex 同样弱，非独有短板)                    |
| P2-14 | Fish 快照缺失、无交互式 shell、输出截断无落盘         | `shell.rs:55,21`；`exec.rs:13`          | Gemini(交互式 shell) / Codex(bounded output)           |

---

## 6. kxen 独有优势清单

以下为在 10 家中 kxen 明确领先或独有的能力（附证据）：

1. **MRM 全局资源调度器**（10 家唯一）：角色路由 + 降级链 + per-provider/全局 semaphore + 60s RPM 滑窗 + 多账号钉选/字典序轮转，一体化（`src-tauri/src/llm/mrm.rs:58-224`）。竞品要么无、要么明确否决(Goose #6615)、要么只有服务端 5h 窗口限额。
2. **多订阅账号池化 + 按角色/任务路由到不同账号**（10 家唯一原生）：竞品订阅 OAuth 普遍单账号单登录态，各竞品该能力均标 UNKNOWN。
3. **四层递进 loop 检测**（覆盖面最完整）：exact/semantic/stagnation/churn 真实接入 4 类运行路径，normalize 数字不折叠避免误杀（`src-tauri/src/agent/loop_detect.rs:13-149`）。
4. **QuickJS 沙箱 workflow**（唯二原生脚本编排引擎之一）：与 Claude Code Dynamic workflows 同构，硬限制(10min/32/64MB/1MB 栈)更收紧（`src-tauri/src/agent/workflow.rs:19-93`）。
5. **OKF 单规范统一 7 类知识 + 引擎级四态分级注入**：竞品普遍是多套割裂子系统（`src-tauri/src/knowledge/render.rs:10-116`）。
6. **glob 条件激活 + mid-turn 系统提示刷新联动**：粒度比 Cline/Claude Code 的 paths 路径范围更细（`render.rs:81-89`+`run.rs:76-82`）。
7. **会话删除兜底蒸馏**（独特触发时机）：删会话节点提炼 0-5 条 note 落盘可审计 md（`distill.rs:57-87`）。
8. **auto_bg 15s 自动前台转后台**：无需模型显式声明 background，比 Crush(>1min) 更激进（`tools/exec.rs:14`）。
9. **rm->trash 透明遮蔽 + trash 语义删除全链路**：竞品普遍无 trash 恢复语义（`tools/shell.rs:83`）。
10. **F1-F5 破坏命令语义分类器**：nested bash -c 递归解包 + sudo 前缀剥离 + 变量未求值检测，独立于沙箱的语义防护层（`tools/safety/eval.rs`）。
11. **hashline 锚点编辑**：chunk fingerprint + find_shifted 自愈 + 免强制 read-before-edit，竞品资料未见等价机制（`tools/fs_tool.rs:89-207`）。
12. **语音本地流式识别 + 多引擎混合**（收窄后独有）：Apple Speech.framework 本地流式(离线可用) + 云转写降级 + Wispr 三层；Claude Code/Codex 听写均为纯云端（`src-tauri/src/voice/mod.rs:104-134`）。注意 PTT 交互形态本身非独有。
13. **角色只读权限编译期单测强约束**（`readonly_roles_cannot_write`）+ 取消令牌三检查点级联 + 终态必落库（`subagent.rs:174-184`；`run.rs:184-187`）。
14. **自研 agent + 原生 Tauri GUI 一体**：Conductor 原生但封装他人 harness，Goose 是 Electron，VK 是 Web 后补壳；kxen 是少数自研 agent + 原生 GUI 组合。

> 注意：优势 8-11、13 属"能力独有性"判断；因缺 OS 沙箱 + ask-user 审批 + 项目信任门(P0/P1)，kxen 整体安全模型仍弱于 Claude Code/Codex/Gemini。dev server 就绪门为"少见/更完整"而非独有(VK 有 URL 自动探测)。多订阅寄生官方 CLI 登录态存在实质 ToS 风险，非可持续设计优势(详见 4.1 风险提示)。

---

## 7. 附录

### 7.1 信息来源（竞品，带协议头）

kxen 事实全部来自本仓库 `src-tauri/src/**` 与 `src/**` 源码逐文件盘点（file:line 已在正文标注），不再重复列举。竞品主要来源如下（每条 URL 均带完整协议头）：

Claude Code：

- https://code.claude.com/docs/en/overview
- https://code.claude.com/docs/en/authentication
- https://code.claude.com/docs/en/agents
- https://code.claude.com/docs/en/workflows
- https://code.claude.com/docs/en/sandboxing
- https://code.claude.com/docs/en/permissions
- https://code.claude.com/docs/en/checkpointing
- https://code.claude.com/docs/en/memory
- https://code.claude.com/docs/en/hooks
- https://code.claude.com/docs/en/mcp
- https://code.claude.com/docs/en/plugins
- https://code.claude.com/docs/en/voice-dictation.md
- https://code.claude.com/docs/en/debug-your-config
- https://github.com/anthropics/claude-code/issues/68911
- https://github.com/anthropics/claude-code/issues/47626

OpenAI Codex：

- https://developers.openai.com/codex/auth
- https://developers.openai.com/codex/concepts/subagents
- https://developers.openai.com/codex/concepts/sandboxing
- https://developers.openai.com/codex/permissions
- https://developers.openai.com/codex/hooks
- https://developers.openai.com/codex/memories
- https://developers.openai.com/codex/use-cases/follow-goals
- https://developers.openai.com/codex/app/worktrees
- https://developers.openai.com/codex/plugins
- https://developers.openai.com/codex/sdk
- https://github.com/openai/codex/blob/HEAD/LICENSE (Apache 2.0)
- https://github.com/openai/codex/issues/11626
- https://github.com/openai/codex/issues/12558
- https://github.com/openai/codex/pull/16114

Gemini CLI：

- https://github.com/google-gemini/gemini-cli
- https://geminicli.com/docs/get-started/authentication/
- https://geminicli.com/docs/core/subagents/
- https://geminicli.com/docs/cli/trusted-folders/
- https://geminicli.com/docs/cli/sandbox/
- https://geminicli.com/docs/cli/gemini-md/
- https://geminicli.com/docs/cli/skills/
- https://geminicli.com/docs/hooks/
- https://github.com/google-gemini/gemini-cli/blob/caa04664/packages/core/src/services/loopDetectionService.ts

OpenCode：

- https://opencode.ai/docs/providers/
- https://opencode.ai/docs/agents/
- https://opencode.ai/docs/permissions/
- https://opencode.ai/docs/rules/
- https://opencode.ai/docs/lsp/
- https://opencode.ai/docs/mcp-servers/
- https://opencode.ai/docs/sdk/
- https://github.com/anomalyco/opencode/issues/21733
- https://github.com/anomalyco/opencode/issues/25254
- https://github.com/anomalyco/opencode/pull/25255
- https://github.com/anomalyco/opencode/pull/32089
- https://env.dev/ai/opencode

Crush：

- https://github.com/charmbracelet/crush
- https://charmbracelet-crush.mintlify.app/cli/login
- https://charmbracelet-crush.mintlify.app/configuration/permissions
- https://charmbracelet-crush.mintlify.app/configuration/mcp
- https://charmbracelet-crush.mintlify.app/guides/context-files
- https://github.com/charmbracelet/crush/blob/main/docs/hooks/README.md
- https://github.com/charmbracelet/crush/blob/main/LICENSE.md (FSL-1.1-MIT)
- https://github.com/charmbracelet/crush/issues/431

Cline/Roo：

- https://docs.cline.bot/cline-overview
- https://docs.cline.bot/getting-started/authorizing-with-cline
- https://docs.cline.bot/features/subagents
- https://docs.cline.bot/cli/agent-teams
- https://docs.cline.bot/core-workflows/checkpoints
- https://docs.cline.bot/customization/cline-rules
- https://docs.cline.bot/sdk/architecture/hub-spoke
- https://docs.cline.bot/sdk/plugins
- https://github.com/cline/cline/pull/10730
- https://github.com/RooCodeInc/Roo-Code

Goose：

- https://github.com/aaif-goose/goose
- https://goose-docs.ai/docs/getting-started/installation/
- https://github.com/aaif-goose/goose/blob/main/documentation/docs/getting-started/providers.md
- https://github.com/aaif-goose/goose/issues/3647
- https://github.com/aaif-goose/goose/issues/6615
- https://github.com/aaif-goose/goose/issues/9407
- https://github.com/aaif-goose/goose/issues/7527
- https://block-goose.mintlify.app/concepts/agents (retry_manager)
- https://goose-docs.ai/docs/guides/context-engineering/using-goosehints/

Conductor：

- https://www.conductor.build/
- https://www.conductor.build/docs/concepts/workflow
- https://www.conductor.build/docs/concepts/git-worktrees
- https://www.conductor.build/docs/reference/checkpoints
- https://www.conductor.build/docs/faq
- https://www.conductor.build/docs/reference/big-terminal-mode
- https://www.conductor.build/docs/reference/mcp
- https://www.conductor.build/changelog/0.44.0-new-sidebar-rebuilt-composer-codex-checkpoints

Vibe Kanban：

- https://github.com/BloopAI/vibe-kanban
- https://vibekanban.com/blog/shutdown
- https://vibekanban.com/docs/workspaces/sessions
- https://vibekanban.com/docs/configuration-customisation/agent-configurations
- https://vibekanban.com/docs/browser-testing
- https://vibekanban.com/docs/integrations/vibe-kanban-mcp-server
- https://github.com/BloopAI/vibe-kanban/issues/1979
- https://github.com/BloopAI/vibe-kanban/issues/1749
- https://github.com/BloopAI/vibe-kanban/pull/2896

补充调研（一手核实）：

- https://code.claude.com/docs/en/voice-dictation.md（Claude Code /voice 纯云端）
- https://github.com/openai/codex/issues/3000（Codex hold Ctrl+M 语音）
- https://github.com/openai/codex/pull/16114（Codex CLI/TUI 语音 2026-03 移除）
- https://github.com/anomalyco/opencode/issues/25254（doom_loop 跨消息漏检，2026-07-23 仍 OPEN）

### 7.2 UNKNOWN 项清单

kxen 侧：

- 实际运行时"重试缺失导致失败即终止"的行为路径为读代码推断（`run.rs:115-119`），未构造失败请求实证。
- release profile 与实测性能指标（包体积/内存/首绘）未在代码盘点范围，Cargo.toml/tauri.conf.json 未读。
- updater plugin 是否接入（main.rs 未见注册），需查 Cargo.toml/tauri.conf.json。
- 目录型 skill 的 move_entry 边界行为（`store.rs:90-100` 未特判 e.dir）。
- distill_on_delete 含真实 LLM 调用的端到端测试是否存在。

竞品侧：

- Codex doctor 自检能力（官方文档未见等价 claude doctor 的诊断命令）。
- Gemini/OpenCode/Crush/Cline/Conductor/Vibe Kanban 的语音听写能力（各 comp 文件未提及，不臆断为"无"）。
- Codex/Gemini/Cline 的 LSP 代码智能细节（comp 未展开）。
- Crush 插件市场、headless 编排参数（`--timeout`/`--role`）是否为上游官方特性（仅第三方 fork 出现）。
- Conductor headless/SDK/企业能力清单（官方文档极简，未公开）。
- Vibe Kanban headless CI token 认证、outbound webhook（均为未合并 feature request）。
- Claude Code/Codex/Gemini 是否有多套 OAuth 订阅账号并行按角色路由的原生能力（各官方文档只描述单一登录态）。
- Gemini Memory 工具的具体存储格式（仅文档索引，正文未抓取）。
- Claude Code doctor 的"通用网络连通性/模型可达"专项检查（仅第三方博客示例提及，官方文档未证实，弱证据不采信）。
