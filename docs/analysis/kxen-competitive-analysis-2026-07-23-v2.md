# kxen 与同类 coding agent harness 功能对比评估（更新版）

- 报告日期：2026-07-23（更新版，反映补全计划 A-G / 提交 824cde9 落地后的当前实现）
- 基线对象：kxen（本地仓库 `file:///Users/xiaobai/Code/SelfCode/kxen`，源码逐文件盘点）
- 对比对象：Claude Code、OpenAI Codex、Gemini CLI、OpenCode、Crush、Cline/Roo、Goose、Conductor、Vibe Kanban（共 9 个产品）
- 上一版：`file:///Users/xiaobai/Code/SelfCode/kxen/docs/analysis/kxen-competitive-analysis-2026-07-23.md`（2026-07-23 采集，驱动了补全计划 A-G）
- 证据口径：kxen 事实一律带 `file:line`（源码实读，未跑真实 app，未跑编译，行为路径推断处标 UNKNOWN）；竞品事实沿用旧报告并标注来源 URL；不确定处标 UNKNOWN，不臆断
- delta 标记：closed 已关闭 / narrowed 已收窄 / still-gap 仍存在 / new 本次新增能力 / newly-identified 本次核实新识别

---

## 1. 执行摘要

### 1.1 kxen 定位

kxen 是一个 aarch64-apple-darwin 专精的原生桌面 coding agent harness（Tauri 2.x + SolidJS 前端，前后端走内嵌本地 WebSocket + JSON-RPC）。它不自研模型，而是寄生四家官方 CLI 已落盘的订阅凭证（Claude / Codex / Grok / Kimi），做多账号池化 + 角色路由 + 并发限流调度（`src-tauri/src/llm/mrm.rs`）。产品形态最接近 Conductor（原生 macOS GUI），但 Conductor 只封装他人 harness，kxen 是自研 agent loop + 原生 GUI 一体，这一组合在 10 家里是少数。

相对旧报告，本次核心结论从「设计完整但运行路径大面积未接线」转为「多条 P0/P1 自治与可靠性缺口已真实闭合、进入运行主链路」。补全计划 A-G 不是装饰：retry / refresh / compact / checkpoint / websearch / approval / trust / workflow_journal 均已接线进真实调用路径而非孤立结构体。但生态维度（MCP / LSP / 插件 / SDK / headless）与 OS 级沙箱两块，本次完全未动，仍是相对头部竞品的根本代差。

### 1.2 相对旧报告最重要的变化：补全计划关闭了哪些 P0/P1

已 closed 的旧 P0/P1（均经代码核实接线真实运行路径，非仅单测）：

- P0 无请求重试/退避/换账号 -> `src-tauri/src/llm/retry.rs` + 接线 `src-tauri/src/agent/agent_loop/run.rs:97-164`（429/5xx/网络类退避 + 同 provider 换账号）
- P0 无 OAuth token 主动刷新 -> `src-tauri/src/auth/refresh.rs:43-100` + 接线 `run.rs:95`、`src-tauri/src/ws/llm_task.rs:148`（anthropic/openai 到期前刷新）
- P0/P1 无上下文压缩 -> `src-tauri/src/agent/compact.rs:1-118` + 自动触发 `run.rs:84-90` + 手动 `/compact`（`llm_task.rs:22-70`）
- P0 goal 自治闭环未接线 -> `run.rs:218-246`（每轮 record_turn 真实驱动预算/阻塞并中断本轮）+ GoalUpdate 双路径 publish
- P0 无 OS 沙箱且无 ask-user 审批的「审批」半 -> `Verdict::Ask` + ApprovalBroker + 前端 ApprovalCard 全链路（`src-tauri/src/tools/safety/rules.rs:8-19`、`src-tauri/src/agent/approval.rs:8-50`、`src-tauri/src/tools/exec.rs:60-104`）
- P1 命令解析绕过面（`||`/换行/反引号/`$()`）-> `src-tauri/src/tools/safety/eval.rs:39,60-81` + 测试实证 `src-tauri/tests/safety_eval.rs:30-39`
- P1 进程组孙进程泄漏 -> `.process_group(0)` 建组 + killpg 组信号（`exec.rs:174-176`、`src-tauri/src/tools/task.rs:92-114`）
- P1 subagent 角色不可自定义 -> `.agents/agents/<role>.md` frontmatter 覆盖（`src-tauri/src/agent/subagent.rs:96-127`）
- P1 workflow 无 resume/journal -> `src-tauri/src/agent/workflow_journal.rs`（派发级缓存续跑）
- P2 Team 不跨进程存活 / cron 纯内存 / GoalUpdate 不 publish / injection_preview 固定空 involved / SessionTree 不可展开 / RoutingSection fallback 只读 / UsageSection stub -> 均已 closed

新增能力（旧报告未提及）：checkpoint/rewind shadow git 真实时间旅行（`src-tauri/src/tools/checkpoint.rs`，代码回滚 + 会话截断联动）、websearch 真实工具（`src-tauri/src/tools/websearch.rs`）、core/trust.rs 项目信任门接入知识注入侧、OS 桌面推送（非前台会话完成弹系统通知）。

### 1.3 当前最重要的 5 个残留差距

1. P0 OS 级沙箱仍完全空白：全库 grep `sandbox-exec|Seatbelt|bwrap|seccomp` 零命中，无进程级隔离，纯字符串规则 + 路径守卫软防护。对标 Claude Code / Codex / Gemini 三家均有 OS 沙箱 + 网络隔离。A-G 计划未涉及。still-gap。
2. P0 MCP 完全缺失：全库 grep `mcp` 零命中，`src-tauri/src/mcp` 目录不存在，仅新增 70 行设计稿（`docs/plans/2026-07-23-mcp-client.md:3` 自述「设计稿待确认，未开始实现」）。9 个竞品全部支持 MCP。still-gap。
3. P1 项目级 hooks 信任门是死代码：`src-tauri/src/core/config.rs:112-118` 保留 `project: Option<&Path>` 参数，但全库 9 处 `Config::load(` 调用无一传入 `Some(project_path)`，merge 逻辑从未触发，hooks 仍固定 `/bin/zsh -c` 执行外部命令（`hooks.rs:82-83`）。narrowed（攻击面因项目 config 不加载而暂不可达，但功能未完成）。
4. P1 无后台持续记忆 consolidation：`src-tauri/src/knowledge/distill.rs:56-87` 仅在会话删除时触发蒸馏，会话进行中不做任何记忆写入/整理。对标 Codex Memories 两阶段后台 pipeline、Claude Code Auto memory。still-gap。
5. P1 Goal::focus() 进程级全局单例，多 session 并发互相误伤：`src-tauri/src/core/goal.rs:205-209` 按 updated_at 取全局唯一焦点 goal，`run.rs:218` 任何 session 的每一轮都推进同一个全局预算/阻塞计数器。newly-identified（接线修复是真实的，但接线后暴露语义粒度缺陷）。对标 Codex `/goal` thread-scoped。

### 1.4 当前 3 个独有优势

1. MRM 全局资源调度器（10 家唯一）：角色路由 resolve + per-provider 并发 semaphore + RPM 滑窗 + config 化降级链 + 账号轮转一体（`src-tauri/src/llm/mrm.rs:48-225`）。本次提交未稀释该优势。补全后新增差异点：retry.rs 把「请求重试」与「账号轮转」耦合为同一容错路径，竞品未见等价公开实现。
2. 四层递进 loop 检测 + LLM 请求四位一体韧性层：loop_detect 四层（exact/semantic/stagnation/churn，`loop_detect.rs:13-122`）接入 4 类运行路径；叠加 retry + backoff + 账号轮换 + OAuth 主动刷新全部接入主循环（`retry.rs` + `refresh.rs` + `run.rs:95,97-164`），是同类中最完整的自治可靠性 + 请求韧性组合。
3. OKF 单规范统一知识系统 + 注入级信任分级：一份 frontmatter 超集统一 project/personal 双 scope x 7 类知识、引擎级四态分级注入（`src-tauri/src/knowledge/render.rs`），本次叠加 core/trust.rs 项目信任门（未信任 project scope 只索引不全文注入），无对标竞品在知识注入层面有等价信任分级。

---

## 2. kxen 当前功能全景（按 6 子系统）

状态口径：implemented = 代码 + 调用链完整；partial = 有实现但存在明确缺口；stub = 结构在但无实际行为；planned/absent = 仅设计文档/枚举变体，代码零落地。delta 相对旧文档基线。

### 2.1 模型接入与订阅认证（authllm）

| 功能                                           | 状态        | 证据 file:line                                                         | delta     |
| ---------------------------------------------- | ----------- | ---------------------------------------------------------------------- | --------- |
| 四源官方凭证探测                               | implemented | `src-tauri/src/auth/probe.rs:26-31,139-259`                            | unchanged |
| 新鲜度比较 + 30min 豁免窗 + 中毒值自愈         | implemented | `src-tauri/src/auth/probe.rs:44-61,66-108`                             | unchanged |
| Keychain 5s 超时保护                           | implemented | `src-tauri/src/auth/probe.rs:35-42`                                    | unchanged |
| 多账号池化（钉选/字典序轮转）                  | implemented | `src-tauri/src/auth/credential.rs:51-82`                               | unchanged |
| auth.json 原子写 + 0600 权限                   | implemented | `src-tauri/src/auth/credential.rs:91-104`                              | unchanged |
| Anthropic OAuth 契约五要素 + 工具名双向重映射  | implemented | `src-tauri/src/llm/anthropic.rs:8-27,180-221`                          | unchanged |
| OpenAI/Codex Responses API 双端点              | implemented | `src-tauri/src/llm/openai.rs:10-11,126`                                | unchanged |
| xAI/Kimi openai-compatible + 自定义双协议      | implemented | `src-tauri/src/llm/client.rs:45-81`                                    | unchanged |
| 模型目录三级源 + 24h TTL + models.dev          | implemented | `src-tauri/src/llm/catalog.rs:60-119,156-198`                          | unchanged |
| 订阅活性 ping 校验                             | implemented | `src-tauri/src/llm/verify.rs:22-57`                                    | unchanged |
| 自研 SSE 帧解析 + 工具调用累积                 | implemented | `src-tauri/src/llm/sse.rs:6-42`；`anthropic_sse.rs`                    | unchanged |
| 图片输入（base64，三 provider 各自块格式）     | implemented | `src-tauri/src/llm/types.rs:14-40`                                     | unchanged |
| OAuth token 主动刷新（接线主循环）             | implemented | `src-tauri/src/auth/refresh.rs:43-100`；`run.rs:95`；`llm_task.rs:148` | closed    |
| 请求失败重试/退避/429特判/换账号（接线主循环） | implemented | `src-tauri/src/llm/retry.rs:1-50`；`run.rs:97-164`                     | closed    |
| MRM 全局资源调度                               | implemented | `src-tauri/src/llm/mrm.rs:48-225`                                      | unchanged |
| doctor 文案与刷新实现一致性                    | implemented | `src-tauri/src/doctor.rs:29,42`；刷新已生效                            | closed    |
| MRM 感知运行时错误（重试换账号复用 MRM 语义）  | partial     | `run.rs` grep `mrm.` 零命中；`mrm.rs:58-96`                            | still-gap |
| 公网图片 URL 输入                              | absent      | `types.rs:30-40`（仅 media_type+data）                                 | still-gap |
| 非流式 / 结构化输出（JSON schema）             | absent      | `anthropic.rs:205` stream 硬编码 true                                  | still-gap |

### 2.2 代理编排与自治（agent）

| 功能                                                  | 状态        | 证据 file:line                                                        | delta            |
| ----------------------------------------------------- | ----------- | --------------------------------------------------------------------- | ---------------- |
| 5 硬编码角色预设 + 权限画像                           | implemented | `src-tauri/src/agent/subagent.rs:83-93`                               | unchanged        |
| 只读角色不含写工具（编译期单测强约束）                | implemented | `subagent.rs:174-184`                                                 | unchanged        |
| subagent 角色可用户自定义（.agents/agents/<role>.md） | implemented | `subagent.rs:96-127,132`                                              | closed           |
| subagent 禁止派孙代理（结构性硬限制）                 | implemented | `subagent.rs:152`；`execute.rs:248-249`                               | unchanged        |
| QuickJS 沙箱 workflow                                 | implemented | `src-tauri/src/agent/workflow.rs:19-179`                              | unchanged        |
| workflow resume/journal（派发级缓存）                 | implemented | `src-tauri/src/agent/workflow_journal.rs:1-62`；`workflow.rs:111-127` | closed           |
| ultracode/ultraplan/ultrareview（纯提示词剧本）       | partial     | `commands.rs:18-20`；`prompt.rs:66-89`                                | unchanged        |
| Goal 8 态状态机 + 三维预算 + 阻塞三次规则             | implemented | `src-tauri/src/core/goal.rs:5-174`                                    | unchanged        |
| goal record_turn 接线主循环                           | implemented | `run.rs:218-246`；`goal_rpc.rs:65-70`                                 | closed           |
| GoalUpdate 事件 publish                               | implemented | `event.rs:10`；`run.rs:226,235`；`goal_rpc.rs:75-77`                  | closed           |
| Agent Teams（spawn/inbox/审批门/依赖解锁/hook 否决）  | implemented | `team/manager.rs:94-186`；`team/tasks.rs:19-61`                       | unchanged        |
| Team 状态跨进程存活                                   | implemented | `team/manager.rs:24-72`（restore，无 remove_dir_all）                 | closed           |
| Cron 调度（持久化到 schedule.json）                   | implemented | `core/schedule.rs:1,21-38`；`main.rs:213`                             | closed           |
| Agent 活动注册表（内存 ring buffer）                  | implemented | `activity.rs:1-9`（自述不持久）                                       | unchanged        |
| 取消令牌三检查点级联 + 子代理继承                     | implemented | `cancel.rs:1-35`；`run.rs:58-61,104-110,183-197`                      | unchanged        |
| approval / ask-user 交互审批档                        | implemented | `approval.rs:1-50`；`rules.rs:16`；`exec.rs:79-104`                   | closed           |
| 四层递进 loop 检测                                    | implemented | `loop_detect.rs:13-122`；`run.rs:199-203`                             | unchanged        |
| goal focus 按 session/任务隔离                        | absent      | `goal.rs:205-209`（进程级全局单例）                                   | newly-identified |

### 2.3 工具执行与安全（tools）

| 功能                                             | 状态        | 证据 file:line                                                                      | delta       |
| ------------------------------------------------ | ----------- | ----------------------------------------------------------------------------------- | ----------- |
| exec 快照 shell（login+rc 回放，无状态并发安全） | implemented | `tools/shell.rs:52-70,73-90`；`exec.rs:68-146`                                      | unchanged   |
| auto_bg 15s 自动前台转后台 + task 注册表         | implemented | `exec.rs:14,118-145`；`task.rs:57-114`                                              | unchanged   |
| dev server 就绪门                                | implemented | `dev_server.rs:39-63,65-134`                                                        | unchanged   |
| Fish 快照捕获（循环覆盖，但路径硬编码单一架构）  | partial     | `shell.rs:22,56`（fish 硬编码 `/opt/homebrew/bin/fish`）                            | narrowed    |
| F1-F5 破坏命令语义分类器                         | implemented | `safety/eval.rs:12-108,114-169`；`rules.rs:47-99`                                   | unchanged   |
| 命令解析防绕过（`                                |             | `/换行/反引号/`$()`）                                                               | implemented | `safety/eval.rs:39,60-81`；`tests/safety_eval.rs:30-39` | closed          |
| OS 级沙箱                                        | absent      | grep `sandbox-exec                                                                  | Seatbelt    | bwrap                                                   | seccomp` 零命中 | still-gap |
| approval/ask-user 审批（Verdict::Ask 全链路）    | implemented | `rules.rs:8-29`；`approval.rs:8-50`；`exec.rs:60-104`；`rpc.rs:271-274`             | closed      |
| 项目信任门（.agents 知识注入侧）                 | implemented | `core/trust.rs:37-76`；`render.rs:31,60-65`；`rpc.rs:83`                            | closed      |
| 项目信任门（.kxen/config.toml 项目级 hooks 侧）  | absent      | `config.rs:112-118`；9 处 `Config::load(` 无一传项目路径                            | still-gap   |
| read hashline 锚点 + edit 双 mode                | implemented | `fs_tool.rs:68-85,109-207`                                                          | unchanged   |
| delete/write 走 trash + .kxen-bak 备份           | implemented | `fs_tool.rs:221-246`                                                                | unchanged   |
| rm->trash 透明遮蔽（仅 rm）                      | partial     | `shell.rs:85-89`                                                                    | unchanged   |
| glob（尊重 .gitignore）+ grep（512KB 上限）      | implemented | `search.rs:1-41`                                                                    | unchanged   |
| webfetch（正则剥 HTML，50k cap）                 | implemented | `webfetch.rs:1-41`                                                                  | unchanged   |
| websearch 工具（DuckDuckGo HTML 检索）           | implemented | `websearch.rs:1-99`；`tools_spec.rs:300-309`；`execute.rs:194-196`                  | closed      |
| hooks 六类事件全覆盖真实调用点                   | implemented | `hooks.rs:43-64`；`main.rs:189`；`llm_task.rs:275`；`rpc.rs:92`；`team/tasks.rs:51` | closed      |
| checkpoint/rewind 时间旅行（shadow bare git）    | implemented | `checkpoint.rs:9-93`；`llm_task.rs:128-138`；`session_ops.rs:17-30`                 | new         |
| 进程组/进程树 kill（killpg 组信号升级）          | implemented | `exec.rs:174-176`；`task.rs:92-114`                                                 | closed      |
| 网络隔离 / 域名 allowlist                        | absent      | webfetch/websearch 均直连 reqwest，无 allowlist                                     | still-gap   |
| 用户可配置 allow 规则语法                        | absent      | `safety/rules.rs`（仅内置硬编码规则族）                                             | still-gap   |

### 2.4 知识与记忆（knowledge）

| 功能                                                             | 状态        | 证据 file:line                                              | delta     |
| ---------------------------------------------------------------- | ----------- | ----------------------------------------------------------- | --------- |
| OKF 双 scope x 7 类统一一棵树 + frontmatter 超集                 | implemented | `knowledge/mod.rs:18-134`；`parse.rs:7-114`；`scan.rs:9-31` | unchanged |
| 注入四态分级（Rules/Notes&memory/索引/Skills 清单）              | implemented | `render.rs:42-109`                                          | unchanged |
| globs 条件激活 + mid-turn 刷新                                   | implemented | `render.rs:54-60,120-128`；`run.rs:45-47,77-80`             | unchanged |
| 多层就近 AGENTS.md                                               | implemented | `render.rs:131-155`                                         | unchanged |
| knowledge 工具（add/list/remove，同 slug 覆盖）                  | implemented | `execute.rs:88-107`；`store.rs:7-51`                        | unchanged |
| 会话删除兜底蒸馏（尾部 12000 字符 best-effort）                  | implemented | `distill.rs:56-87`；`rpc.rs:123`                            | unchanged |
| skill 渐进披露（递归 cap3 + 去重 + disable_model_invocation）    | implemented | `skills.rs:7,38-78`；`execute.rs:216-244`                   | unchanged |
| move_entry 跨 scope 晋升/降级                                    | implemented | `store.rs:90-100`；`ops.rs:95`                              | unchanged |
| 第三方规则格式互操作（AGENTS/CLAUDE/GEMINI/.cursorrules 根文件） | partial     | `scan.rs:16-27,122-133`（仅根级单文件，无目录式格式）       | narrowed  |
| 记忆动态检索（involved 分词相关性 top-K）                        | partial     | `render.rs:10-28,78-93`（无 tag/usage/语义维度）            | narrowed  |
| injection_preview 感知真实 involved 文件集                       | implemented | `ops.rs:98-108`；`llm_task.rs:270`；`main.rs:49`            | closed    |
| 项目信任门（未信任 project scope 只索引不全文注入）              | implemented | `trust.rs:41-53`；`render.rs:31,47-52`；`rpc.rs:83`         | new       |
| 后台持续记忆 consolidation                                       | absent      | `distill.rs` 仅会话删除触发                                 | still-gap |
| rules/reference @import 模块化                                   | absent      | 未见 @import 机制                                           | still-gap |
| 组织/managed policy 强制层                                       | absent      | 仅 project+personal 双 scope                                | still-gap |

### 2.5 会话与 UI 体验（ui）

| 功能                                                  | 状态        | 证据 file:line                                                                          | delta     |
| ----------------------------------------------------- | ----------- | --------------------------------------------------------------------------------------- | --------- |
| composer 三触发与选择器                               | implemented | `src/components/composer/TextComposer.tsx`（本次未触及）                                | unchanged |
| 语音 PTT 双引擎（本地流式 + 云降级 + Wispr）          | implemented | `src-tauri/src/voice/mod.rs`（本次未触及）                                              | unchanged |
| SessionTree（分组/拖拽/内联重命名/运行态脉冲点）      | implemented | `src/components/SessionTree.tsx:1-236`                                                  | unchanged |
| SessionTree 单组超 5 条可展开                         | implemented | `SessionTree.tsx:29,81-88,137-138,184-193`                                              | closed    |
| sessionFork + 编辑重发 fork + rerun                   | implemented | `src/lib/session-actions.ts:1-48`                                                       | unchanged |
| checkpoint/rewind 时间旅行（代码+对话双回退）         | implemented | `checkpoint.rs:1-114`；`session_ops.rs:17-30`；`Session.tsx:210-215`；`UserItem.tsx:24` | closed    |
| CommandPalette（Cmd/Ctrl+K 三路混合搜索）             | implemented | `src/components/CommandPalette.tsx`（本次未触及）                                       | unchanged |
| NotificationCenter（应用内铃铛 + 未读角标）           | implemented | `src/components/NotificationCenter.tsx:1-40`                                            | unchanged |
| OS 级/跨会话桌面推送通知                              | implemented | `main.rs:53,115,168-181`；`rpc.rs:139-143`；`state.ts:58,65`                            | closed    |
| DockWorktree（list/create/remove）+ worktree 隔离派发 | implemented | `src/components/DockWorktree.tsx:19-135`                                                | unchanged |
| DockWorktree 增强：脏文件计数 + 一键切换              | implemented | `DockWorktree.tsx:15-17,25-36,88-98`                                                    | new       |
| 并行 workspace 中心看板                               | absent      | `RightColumn.tsx:7,76`（仍侧边 Dock 单列列表）                                          | still-gap |
| 改动快照面板 + 后台任务 dock + goal dock              | implemented | `src/components/Dock.tsx`；`snapshot.rs`（本次未触及）                                  | unchanged |
| ThinkingOrb 四态动画 + 安全 markdown/mermaid 管线     | implemented | `ThinkingOrb.tsx`；`markdown.ts`（本次未触及）                                          | unchanged |
| 审批卡片 ApprovalCard（Verdict::Ask 交互审批档）      | implemented | `approval.rs:1-50`；`exec.rs:62,80-97`；`ApprovalCard.tsx:1-38`；`Session.tsx:273-279`  | new       |
| Settings 用量与统计节（真实 RPC，但进程内存态非持久） | implemented | `UsageSection.tsx:1-65`；`ops.rs:110-130`                                               | closed    |
| RoutingSection 降级链 fallback 编辑                   | implemented | `RoutingSection.tsx:199-222`；`settings.rs:50,64-65`                                    | closed    |

### 2.6 生态与扩展性（ecosystem）

| 功能                                    | 状态        | 证据 file:line                                                                         | delta              |
| --------------------------------------- | ----------- | -------------------------------------------------------------------------------------- | ------------------ |
| MCP client                              | planned     | grep `mcp` 零命中；`docs/plans/2026-07-23-mcp-client.md:3`；`src-tauri/src/mcp` 不存在 | unchanged          |
| LSP 代码智能                            | planned     | grep `lsp` 零命中；`docs/plans/2026-07-23-lsp-diagnostics.md:3`                        | unchanged          |
| 插件/marketplace 打包分发               | planned     | `docs/plans/2026-07-23-plugin-system.md:3`；`prd.md:70` 列非目标                       | unchanged          |
| SDK/编程接口                            | absent      | `src-tauri/src/bin` 不存在；Cargo.toml 无 [[bin]]；invoke_handler 仅 ws_port           | unchanged          |
| headless/CI（CLI 子命令/GitHub Action） | absent      | `main.rs` 无 clap/args；`prd.md:70` 列非目标                                           | unchanged          |
| websearch（DuckDuckGo HTML）            | implemented | `websearch.rs:1-99`；`tools_spec.rs:305`；`execute.rs:194-196`                         | narrowed（原缺失） |
| 定价（BYO 订阅零计费）                  | implemented | `auth/probe.rs`；无计费/转售代码                                                       | unchanged          |
| 开源协议                                | absent      | 根目录无 LICENSE；`package.json:3` `"private": true`；`prd.md` 自称开源存矛盾          | unchanged          |
| 跨平台                                  | absent      | `tauri.conf.json` bundle.targets 仅 `["dmg"]`；`prd.md:70` 列非目标                    | unchanged          |

---

## 3. 竞品概览表（9 个产品）

沿用旧报告，本次核实竞品事实无变化。

| 产品         | 形态                                                  | 模型接入                                                                        | 编排                                                                       | 开源与定价                                              |
| ------------ | ----------------------------------------------------- | ------------------------------------------------------------------------------- | -------------------------------------------------------------------------- | ------------------------------------------------------- |
| Claude Code  | 终端 CLI + IDE 扩展 + Desktop + Web + Cloud           | 单模型族多 provider（Bedrock/Vertex/Foundry/gateway），env var 优先于订阅 OAuth | 四层（subagent/agent view/teams 实验/dynamic workflows 上限 1000 并发 16） | CLI 闭源；SDK 双协议；Pro/Max/Team/Enterprise           |
| OpenAI Codex | Rust CLI（TUI）+ IDE 扩展 + ChatGPT App + cloud       | 单模型族 + 实验 Bedrock；Sign in with ChatGPT OAuth / API key                   | 原生 subagent V2 + Agents SDK（经 MCP）+ cloud best-of-N                   | Apache 2.0；ChatGPT 订阅 / API key                      |
| Gemini CLI   | 纯终端 CLI（IDE 靠 Companion/ACP）                    | 仅 Google 生态；OAuth / API Key / Vertex 三选一                                 | 本地 subagent + A2A 远程 agent；无声明式 workflow 引擎                     | Apache 2.0；CLI 免费，付费在 Google AI Pro/Ultra 或 API |
| OpenCode     | TUI + 桌面 App（Beta）+ IDE 扩展，client/server       | 75+ provider；各 provider 独立 OAuth，Claude Pro/Max 已被封锁                   | Agent primary/subagent + 后台 subagent（实验）；无原生 workflow 引擎       | MIT；软件免费，托管网关可选                             |
| Crush        | 终端 TUI，无 GUI                                      | 近 30 provider + Catwalk 自动同步；hyper/copilot 两条 OAuth                     | 仅 2 硬编码 subagent，不可自定义；无 workflow 引擎                         | FSL-1.1-MIT；免费，成本来自 provider                    |
| Cline/Roo    | Cline: VS Code+CLI+SDK+Kanban+菜单栏；Roo 转 Zoo Code | 15+ provider（含 openai-codex 复用 ChatGPT）；OAuth+ClinePass+BYOK              | Cline 三层；Roo Orchestrator+Modes                                         | Cline Apache 2.0；ClinePass / BYOK                      |
| Goose        | Electron 桌面 + CLI + REST server                     | 15-30+ provider；不自做 Anthropic OAuth，走 CLI/ACP pass-through                | subagent + sub-recipe（均不可再嵌套）；无脚本引擎                          | Apache 2.0；免费，BYO provider                          |
| Conductor    | 原生 macOS 桌面 App（封装 CC/Codex/Cursor/OpenCode）  | 透传底层 harness，BYO 订阅，不计费不转售                                        | 核心单元=workspace（=worktree+分支）；无角色化 subagent/无脚本引擎         | 闭源；应用免费，Cloud beta 定价未公开                   |
| Vibe Kanban  | 本地 Web 服务 + 后补 Tauri 壳                         | 透传 10+ agent CLI，不管凭据                                                    | workspace+session 多实例 + 每 attempt 独立 worktree；无角色化/无脚本引擎   | Apache 2.0；本地免费，Cloud 已关停                      |

---

## 4. 七维度逐项对比

### 4.1 维度一：模型接入与订阅认证

kxen 当前：4 家内置订阅 provider + 双协议自定义通道；读官方 CLI 落盘凭证导入 auth.json，请求侧伪装 claude-cli UA 直连订阅端点（ToS 风险仍在）；多账号池化 + MRM 角色路由/并发/RPM/降级链；models.dev 三级 catalog；图片仅 base64。824cde9 新增两项并接入 agent_loop 主循环：OAuth token 主动刷新（`refresh.rs`）与请求失败重试+退避+换账号（`retry.rs`）。

| 维度                 | kxen 当前状态                                                            | 竞品基线                                                   | delta     |
| -------------------- | ------------------------------------------------------------------------ | ---------------------------------------------------------- | --------- |
| 请求重试/退避        | 429/5xx/网络类退避 + 同 provider 换账号（`run.rs:97-164`）               | Claude Code RETRY_WATCHDOG；OpenCode RETRY_MAX_ATTEMPTS=10 | closed    |
| OAuth token 主动刷新 | anthropic/openai refresh grant + 5min 缓冲 + 去重（`refresh.rs:43-100`） | Codex/Gemini/Cline/Crush 均有                              | closed    |
| MRM 感知运行时错误   | 换账号绕过 mrm.resolve/acquire，不做跨 provider/角色降级                 | Gemini ModelAvailabilityService 全局降级                   | narrowed  |
| 内置 provider 广度   | 仅 4 家 + 双协议自定义                                                   | Claude 多云；OpenCode 75+；Crush 近 30+                    | still-gap |
| 非流式/结构化输出    | stream 硬编码 true，无 JSON schema                                       | Codex SDK / Gemini output_schema                           | still-gap |
| 公网图片 URL 输入    | 仅 base64                                                                | 主流 API 支持 URL                                          | still-gap |

gaps：

- P1 MRM 全局调度与运行时重试换账号仍是两套独立机制：`run.rs` grep `mrm.` 零命中，`retry.rs::next_account` 仅在 AuthStore 内按同 provider 找下一账号，不经 `mrm.rs:58-96` 的 resolve/acquire，换账号后不受并发 semaphore/RPM 滑窗约束，也不触发跨 provider/跨角色降级。narrowed（旧文档为「完全不感知运行时错误」，现至少有同 provider 账号级重试兜底，但核心语义空隙未闭合）。
- P1 内置订阅 provider 广度仍仅 4 家（`client.rs:45-81`）。still-gap。
- P1 无非流式/结构化输出（`anthropic.rs:205`）。still-gap。
- P2 重试上限与退避封顶偏保守：`retry.rs` MAX_ATTEMPTS=3、退避 800ms<<attempt+抖动，未见封顶设置；对比 Claude Code 退避封顶 5min、OpenCode 60s 封顶。newly-identified（旧基线无重试，落地后才可对比）。
- P2 公网图片 URL 输入仍未支持。still-gap。

advantages：

- 重试与账号轮转一体设计：`retry.rs` 把 next_account 与退避重试耦合为同一容错路径，接线 `run.rs:97-164`；对照 Claude Code/OpenCode/Goose 均只做同账号重试，未见竞品把两者做成同一容错路径的公开实现。new。
- OAuth 刷新已补齐至订阅类竞品同等身位（`refresh.rs:43-100`）：旧文档列为对 Codex/Gemini/Cline/Crush 的 P0 劣势，现已追平（xai/kimi 无公开刷新端点仍委托官方 CLI，已知限制）。
- 多订阅账号池化 + 角色路由 + 降级链一体（`mrm.rs:48-225`）：10 家唯一原生能力，本次未稀释。
- 官方 CLI 凭证零迁移复用 + 新鲜度自愈（`probe.rs`）。

风险提示（沿用旧报告）：kxen 仍以伪装 `claude-cli` UA 的第三方客户端身份直连订阅端点，与 OpenCode 被封锁、Goose 因 ToS 拒绝合并的方案同属一类，为实质性 ToS 风险，不应表述为设计优势。

### 4.2 维度二：代理编排与自治

kxen 当前：5 硬编码角色现支持 `.agents/agents/<role>.md` frontmatter 覆盖；goal record_turn 接线主循环（`run.rs:218-246`）真实驱动预算/阻塞并可中断；GoalUpdate 双路径 publish；workflow 派发级 journal 缓存续跑；Agent Teams 跨进程 restore；cron 持久化。MRM 与 4 层 loop 检测两条护城河不变。

| 维度                | kxen 当前状态                                             | 竞品基线                                          | delta                                        |
| ------------------- | --------------------------------------------------------- | ------------------------------------------------- | -------------------------------------------- |
| subagent 角色自定义 | .agents/agents/<role>.md 覆盖 permission/max_turns/prompt | Claude Code frontmatter 全维度；Codex subagent V2 | narrowed（基础能力 closed，粒度仍窄）        |
| goal 运行时驱动     | record_turn 接线主循环，预算/阻塞中断本轮                 | Codex `/goal` thread-scoped 运行时驱动            | closed（接线）+ newly-identified（全局单例） |
| workflow resume     | 派发级缓存（role+prompt 命中即跳过 dispatch）             | Claude Code journal 确定性完整重放                | narrowed                                     |
| Team 跨进程存活     | restore 重建 config/task                                  | Cline Teams 任务板/mailbox 持久化                 | closed                                       |
| cron 持久           | schedule.json 落盘 + drain_due 驱动                       | Cline 持久 cron + hub-spoke daemon                | narrowed（cron closed，daemon still-gap）    |
| MRM 资源调度        | 角色路由 + 并发 + RPM + 降级链一体                        | 竞品普遍无 harness 级调度                         | 优势不变                                     |

gaps：

- P1 Goal 生命周期是进程级全局单例，不分 session/任务：`goal.rs:205-209` focus() 取全局唯一焦点 goal，`run.rs:218-246` 任何会话每一轮都推进同一个全局预算/阻塞计数器。对标 Codex `/goal` thread-scoped。newly-identified。
- P2 subagent 结构性禁止派孙代理：`subagent.rs:152` 子 AgentContext.mrm=None，`execute.rs:248-249` 早退。设计层硬限制非 bug，多数竞品对深度嵌套也持谨慎态度（Goose sub-recipe 同样不可再嵌套）。newly-identified。
- P2 持久后台会话/hub-spoke daemon 常驻能力仍未见改动（cron 持久化子项已 closed）。narrowed（P1 降 P2）。
- P2 Agent 活动注册表仍纯内存 ring buffer（`activity.rs:1-9`）。still-gap。
- P2 subagent 角色自定义粒度窄于 Claude Code/Codex，无 per-role model/effort/reasoning。narrowed。
- P2 workflow resume 粒度是派发级缓存，非完整确定性 journal 重放。narrowed。
- P2 无 A2A/远程 subagent。still-gap。

advantages：

- MRM 是 10 家中唯一的 harness 级全局资源调度器（`mrm.rs:48-225`），本次未改动，优势未稀释。
- 四层递进 loop 检测仍是最完整的自治可靠性防护（`loop_detect.rs:13-122` + `run.rs:199-203`）：对比 Claude Code 无统一 loop 防护层、OpenCode doom_loop 曾漏检 1827 次、Goose 无声死循环 bug。
- 角色只读权限具备编译期单测强约束（`subagent.rs:174-184`）。
- 取消令牌三检查点级联 + 子代理继承 + 终态必落库（`cancel.rs` + `run.rs:58-197`）。
- QuickJS 沙箱 workflow 补齐 resume 后，与 Claude Code Dynamic workflows 是仅存的唯二具备 resume 的原生脚本编排引擎（尽管粒度不同）。

相对旧文档变化：旧 P0「goal record_turn 未接线」已真实修复（`run.rs:218-246`），但核对同时新识别 Goal::focus() 进程级全局单例问题，故仍列 P1；「subagent 不可自定义」「workflow 无 resume」基础能力 closed 降为 P2；「Team 不跨进程」「GoalUpdate 未 publish」完全 closed。

### 4.3 维度三：工具执行与安全

kxen 当前：无 OS 级沙箱，防线仍是命令字符串规则 + 路径守卫 + rm 遮蔽软防护，但审批链路从「完全没有」变为 `Verdict::Ask` + ApprovalBroker + 前端卡片全链路；命令解析绕过面补齐并有测试实证；进程组 kill 升级 killpg。项目信任门半闭合：知识注入侧已挂钩 trust.rs，项目级 hooks 侧仍是死代码。

表 1：沙箱与权限审批

| 维度               | kxen 当前状态                                       | 竞品基线                                                                      |
| ------------------ | --------------------------------------------------- | ----------------------------------------------------------------------------- |
| OS 级沙箱          | 无（grep 零命中）                                   | Claude Seatbelt/bwrap；Codex +seccomp+Windows；Gemini 6 profile/Docker/gVisor |
| 网络隔离           | 无（webfetch/websearch 直连）                       | Codex 默认关网+allowlist；Claude 域名 allowlist；Gemini 容器隔离              |
| 权限审批档位       | Allow/Deny/Recoverable + Ask 共 4 态（仅 1 档 Ask） | Claude 七档 + 规则语法；Codex 三档 x 三档；Gemini TOML 策略引擎               |
| 破坏命令结构化防护 | F1-F5 分类器 + 绕过面已封闭                         | 各家沙箱兜底                                                                  |
| 项目信任门         | 知识注入侧 closed，hooks 侧死代码                   | Codex/Gemini Trusted Folders；Goose recipe 信任弹窗                           |

gaps：

- P0 OS 级沙箱仍完全空白：grep `sandbox-exec|Seatbelt|bwrap|seccomp` 零命中。A-G 未涉及，是本维度对前三家竞品的根本代差。still-gap。
- P1 审批链路已从 0 到 1，但仍单档：`rules.rs:8-29` ASK_PATTERNS 仅覆盖 push --force/裸 reset --hard/sudo，仅 1 档 Ask。旧 P0 具体风险已闭合且端到端可审查，但审批粒度（1 档 vs 竞品 3-7 档）仍是明显差距。narrowed（P0 降 P1）。
- P1 项目级 .kxen/config.toml 的 hooks 信任门仍是死代码：`config.rs:112-118` 保留 project 参数，9 处 `Config::load(` 无一传入 `Some(project_path)`，merge 从未触发，hooks 仍固定 `/bin/zsh -c`（`hooks.rs:82-83`）。旧文档具体攻击路径因项目 config 不加载而不可达，但功能未完成。narrowed。
- P1 网络隔离/域名 allowlist 仍缺失，且新增 websearch 直连外网，网络面略扩大。still-gap。
- P1 命令解析绕过面（`||`/换行/反引号/`$()`）已补齐（`eval.rs:39,60-81` + 测试）。closed。
- P2 进程组/进程树 kill 已升级 killpg 组信号（`exec.rs:174-176` + `task.rs:92-114`）。closed。
- P2 用户可配置 allow 规则语法仍缺失。still-gap（相对优先级下调）。
- P2 Fish shell 快照循环已修复，但二进制路径硬编码单一架构（Intel Mac 探测失败）。narrowed。
- P2 输出截断无落盘兜底 / 后台任务纯内存不持久。still-gap（不在 A-G 范围）。

advantages：

- checkpoint/rewind shadow git 真实时间旅行（新增能力）：`checkpoint.rs:9-93` 独立 shadow bare repo，reset_to 走真实 `git reset --hard`，`session_ops.rs:17-30` 联动会话消息截断；对标 Cline/Roo shadow git、Conductor checkpoints，kxen 差异点是代码回滚 + 会话截断联动为单一操作。new。
- .agents 知识注入项目信任门已挂钩（`trust.rs:37-76` + `render.rs:31,60-65`）：对标 Gemini Trusted Folders 思路，覆盖面仅限知识注入侧。
- F1-F5 分类器 + 解析绕过面已封闭，双重防护强度提升（`eval.rs:12-169`）：无 OS 沙箱前提下字符串层防护达补全前理论上限。
- auto_bg 15s 自动前台转后台（unchanged）。
- rm->trash 透明遮蔽 + trash 语义删除全链路（unchanged）。
- hooks 六类事件全覆盖真实调用点（notification/stop/session_start 等，`hooks.rs:43-64` + 多处调用点）。closed。

相对旧文档变化：审批链路旧 P0 收窄 P1；命令解析绕过面旧 P1 closed；进程组 kill 旧 P1 closed；新增 checkpoint/rewind。项目信任门仍半成品；OS 沙箱、网络隔离、用户可配置规则语法三项 A-G 完全未触及。

### 4.4 维度四：会话与 UI 体验

kxen 当前：仍是单一原生 Tauri macOS GUI（仅 Apple Silicon），无 CLI/TUI/IDE/Web 形态。824cde9 做了三处真实闭环：checkpoint/rewind、非前台会话 OS 桌面推送、Verdict::Ask 交互审批卡片。SessionTree 单组超 5 条可展开。最大剩余缺口未变：并行 workspace 中心看板。

gaps：

- P1 无并行 workspace 中心看板视图：DockWorktree 新增脏文件计数与切换（`DockWorktree.tsx:15-98`），但仍嵌在右栏侧边 Dock 内（`RightColumn.tsx:7,76`），本质单列列表，文案改「并行看板」但结构未变。对标 Conductor/Vibe Kanban/Cline Kanban。still-gap。
- P2 checkpoint/rewind 核心闭环已具备，但无分档模式（仅代码+对话一并回退）、无多档快照浏览器/回退前 diff 预览、未与 sessionFork 打通。对标 Claude Code 100 快照 + /rewind 分档、Cline/Roo 3 档恢复。narrowed（P1 降 P2）。
- P2 后台/多会话可见性：OS 推送缺口已闭环（`main.rs:53,168-181` + `rpc.rs:139-143`），但点击系统通知是否深链跳转 UNKNOWN。narrowed（P1 基本关闭）。
- P2 composer 附件/diff viewer 行内评论回传/公开分享链接/UI 内 PR 流程仍缺（本次未触及）。still-gap。
- P2 SessionTree 单组超 5 条已可展开（`SessionTree.tsx:29,81-193`）。closed。

advantages：

- 自研 agent + 原生 GUI 一体设计（Tauri，非 Electron/Web 壳）。
- composer 一体化程度高：@//#三触发 + 每会话草稿 + IME 锁窗 + 路由配置直接放进输入区（`TextComposer.tsx`）。
- 语音双引擎（本地流式 + 云降级）仍是唯一混合方案（`voice/mod.rs`）。
- 会话时间线原生嵌入交互式审批（Verdict::Ask ApprovalCard，`approval.rs:1-50` + `ApprovalCard.tsx:1-38`），同一 broker 复用于项目信任门。new。

相对旧文档变化：checkpoint/rewind 从完全没有变为 shadow git 完整闭环（P1 收窄 P2）；后台 OS 推送 P1 基本关闭；并行 workspace 中心看板仍是最大 P1 缺口；SessionTree 展开 P2 已解决；Verdict::Ask 审批卡片作为会话新交互形态落地。

### 4.5 维度五：知识与记忆

kxen 当前：OKF 单规范统一 project/personal 双 scope x 7 类、注入四态分级、globs + mid-turn、多层就近 AGENTS.md、knowledge 工具、会话删除蒸馏、skill 渐进披露、move_entry，824cde9 均未改动。本次动了三处：scan.rs 根级规则文件互操作扩到 4 种、render.rs notes/memory 改为 involved 分词 top-K、ops.rs injection_preview 读真实 session_involved。新增 core/trust.rs 项目信任门接入 render.rs。

gaps：

- P1 无后台持续记忆 consolidation：`distill.rs:56-87` 仅会话删除时触发，会话进行中不做任何记忆写入/整理。对标 Codex Memories 两阶段后台 pipeline、Claude Code Auto memory。still-gap。
- P1 第三方规则格式互操作仅限根级单文件，不含目录式格式与 personal scope：`scan.rs:16-27` 仅认根目录 AGENTS/CLAUDE/GEMINI/.cursorrules 单文件，未覆盖 .clinerules/、.cursor/rules/*.mdc、.github/copilot-instructions.md。对标 Crush 全兼容、Cline/Roo 目录格式。narrowed（旧为完全空白）。
- P2 记忆检索仅关键词分词打分，无 tag/usage/语义维度：`render.rs:10-28,78-93` 只按 involved 路径分词计数命中，无 usage_count/last_usage 排序。对标 Codex Memories、Goose Memory Extension。narrowed。
- P2 无组织/managed policy 强制层：新增 trust.rs 是执行安全信任判定，非组织策略强制层。still-gap。
- P2 rules/reference 无 @import 模块化组合。still-gap。

advantages：

- OKF 单规范统一双 scope x 7 类知识（`mod.rs:18-134`）：对比 Codex（AGENTS.md + 独立 Memories 两套）、Goose（.goosehints + 独立 Memory Extension 两套）的割裂设计。
- 注入四态分级为引擎级行为，叠加新增项目信任门做注入级安全分级（`trust.rs:41-53` + `render.rs:31,47-52`）：无对标竞品在知识注入层面有等价信任分级。new。
- injection_preview 已打通真实会话 involved 文件集，读写两端闭环验证（`ops.rs:98-108` + `llm_task.rs:270`）。closed。
- glob 条件激活 + mid-turn 刷新联动（`render.rs:54-60` + `run.rs:45-47`），粒度优于 Cline/Claude Code 的 paths 范围。
- 多层就近 AGENTS.md 动态就近（`render.rs:131-155`），对比 Codex/Claude Code 启动静态全量拼接。
- 会话删除兜底蒸馏 + 结构化 note + scope 晋升 + skill 加载工程化防护，仍是独有组合。

相对旧文档变化：第三方根级规则互操作从仅 AGENTS.md 扩到 4 种（P1 narrowed，目录式格式仍空白）；记忆检索从全文注入改为确定性 top-K（P1 narrowed，无 tag/usage/语义）；injection_preview 从固定空 involved 变真实文件集（P2 closed）；新增 trust.rs 信任门。旧另一 P1「无后台持续记忆 consolidation」未改动，是本维度当前最大未收敛缺口。

### 4.6 维度六：可靠性与防失控

kxen 当前：四层递进 loop 检测仍是运行时强制层；Goal 三维预算 + 阻塞三次规则从「仅设计/仅 RPC」变为真实接入主循环，超预算/连续阻塞会中断当前轮并写终态；新增自动上下文压缩（超窗 80% 蒸馏，LLM 不可用降级首尾截断）+ 手动 /compact；新增 LLM 请求重试 + OAuth 主动刷新（接线主循环）；新增 shadow git per-turn checkpoint + rewind 真回滚。doctor 仍只覆盖凭据一维；loop 检测器各自独立实例；Goal::focus() 进程级全局单例。

gaps：

- P1 doctor 自检仍只覆盖凭据一维：`doctor.rs:24-60` 只遍历 auth::probe::RULES 判定凭据三态，无安装完整性/settings 合法性/权限规则遮蔽/MCP 健康/上下文体积告警等。对标 claude doctor 9 类以上。still-gap。
- P1 Goal::focus() 进程级全局单例，多 session 并发时预算/阻塞互相误伤：`goal.rs:205-209` 按 updated_at 取唯一焦点，`paths.rs:30-32` goals_dir 不按 session 分区，`run.rs:218` 任何 session 每一轮都读同一个全局 focus goal。newly-identified（旧文档因 record_turn 未接线此缺陷不可观测，接线后暴露）。
- P2 doctor "will refresh on next call" 文案与刷新保证仍有缝隙：`doctor.rs:29,42` 固定输出该文案，`refresh.rs:43-100` ensure_fresh 确会在下次调用前刷新，但 doctor_report 本身不触发/不回写，刷新失败场景仍会误导。narrowed（旧为纯 stub，现有真实刷新背书）。
- P2 loop 检测各自独立实例，跨 agent 协同循环无法检测：`loop_detect.rs:49` 在主循环/子代理/team 成员各自实例化互不共享（`context.rs:35`、`subagent.rs:163`、`team/member_loop.rs:125`）。9 家竞品均未提及跨 agent 协同循环检测，非独有短板。still-gap。

advantages：

- LLM 请求四位一体运行时韧性层（重试 + 退避 + 账号轮换 + OAuth 主动刷新）全部接入主循环：`retry.rs:1-95` + `refresh.rs:1-131` + `run.rs:95,97-164`。旧文档记录 OpenCode 有自动摘要死循环 bug、其余竞品 UNKNOWN；kxen 现是唯一有完整记录的四合一韧性实现。new。
- 上下文自动压缩 + 手动双通道，带降级兜底：`compact.rs:1-118`（80% 阈值 + LLM 蒸馏 + 不可用降级首尾截断而非硬失败）+ `run.rs:84-90` + `llm_task.rs:22-70`。旧文档将「无上下文压缩」列为最重要 5 个差距之一，现已实质关闭，降级兜底优于部分竞品静默失败。closed。
- Goal 状态机形式化完整度反超 Codex，且「唯一短板未接线」已消除：`goal.rs:5-174`（8 态 + 显式迁移表 + 三维预算 + 阻塞三次规则 + complete 强制 evidence）+ `run.rs:218-246` 真实接入 + GoalUpdate 双路径 publish。进程级隔离粒度问题见 gaps，不影响单 session 场景真实生效。closed。
- shadow-git checkpoint 默认自动打点 + 代码/对话双维度真回滚：`checkpoint.rs:1-114` + `session_ops.rs` 会话截断联动。对标 Gemini Checkpointing 默认关、Cline/Roo 嵌套 git 静默禁用 bug，kxen 默认开启且双回滚同一操作内完成。new。
- 四层递进 loop 检测覆盖面仍强于多数竞品（`loop_detect.rs:13-122`，本次未改动）。

相对旧文档变化：旧列为最重要 5 个差距之一的预算控制未接线与无上下文压缩两个 P0/P1 均在 A-G 真实关闭；新增 LLM 请求重试/backoff/账号轮换与 shadow-git checkpoint/rewind，闭合旧 P1 两条；但主循环接入 Goal::focus() 后新暴露进程级全局单例缺陷（唯一新识别实质问题）；doctor 单维度覆盖不足与 loop 检测器互不聚合维持旧判断。

### 4.7 维度七：生态与扩展性

kxen 当前：核心结论未变，无 MCP、无 LSP、无插件市场、无 SDK、无 headless/CI，hooks 仍为 partial（六类事件已补但仍单一 shell handler），非开源（私有仓库 + 无 LICENSE）、仅单平台，BYO 订阅零计费不变。唯一实质变化是 web 检索：`websearch.rs` 新增 DuckDuckGo HTML 检索并注册为常规工具、完成执行分发，从旧「未实现」收窄为「已落地但深度落后头部竞品」。

gaps：

- P0 MCP 完全缺失：全库 grep `mcp` 零命中，`src-tauri/src/mcp` 不存在，仅新增 70 行设计稿。9 个竞品全部支持 MCP。still-gap（仅设计稿，0 行代码）。
- P1 LSP/代码智能缺失：grep `lsp` 零命中，仅 46 行设计稿。对标 OpenCode 约 40 种语言 LSP、Crush lsp_* 工具。still-gap。
- P1 无 SDK/编程接口：`src-tauri/src/bin` 不存在，invoke_handler 仅 ws_port，kxen_app 是内部 lib crate。对标 Claude Code/Codex/OpenCode/Cline SDK。still-gap。
- P1 无 headless/CI 模式：`main.rs` 无 clap/args，`prd.md:70` 明文列非目标。对标 Claude Code -p/GitHub Action、Goose headless。still-gap（产品定位主动排除，但对外部结论不变）。
- P1 无插件/marketplace 系统：`prd.md:70` 列非目标；`main.rs:129-130` 的 tauri plugin 是官方 runtime plugin，与 agent 插件市场无关。still-gap（刻意排除）。
- P1 web 检索能力（原判缺失，现已实质收窄）：`websearch.rs` DuckDuckGo HTML 检索 + `tools_spec.rs:305` 注册常规工具（非 deferred）+ `execute.rs:194` 执行分发 + 单测。但仍落后头部竞品：无 sourcegraph 式代码检索、无深度研究子代理、依赖 DuckDuckGo HTML 页面结构（脆弱）。narrowed（非完全关闭）。
- P2 hooks 事件与 handler 覆盖不足：`hooks.rs` 仅 pre/post_tool_use，硬编码单一 shell handler。对标 Claude Code 30 事件四类 handler。still-gap（按本维度旧口径；注：其他子系统盘点将 hooks 六类事件计入 tools 维度 closed，此处沿旧文档口径判定）。
- P2 非开源/无 LICENSE/仅单平台：根目录无 LICENSE，`package.json:3` `"private": true`，`prd.md` 自称开源存矛盾，`tauri.conf.json` 仅 dmg targets。still-gap。

advantages：

- BYO 多订阅账号池化 + 按角色/任务路由（`auth/probe.rs` + MRM）：10 家唯一原生，竞品普遍 UNKNOWN。
- 自定义 provider 双协议 + 端点模型清单，覆盖长尾自建场景。
- 注：websearch.rs 落地是差距收窄而非新增优势——DuckDuckGo HTML 抓取深度仍落后 Crush/OpenCode/Gemini，不构成领先项。

相对旧文档变化：web 检索从完全未实现收窄为已落地但深度落后；MCP/LSP/插件三项从零证据补充为「有设计稿但零代码」，严重度不变；headless/CI、插件市场、跨平台三项经 `prd.md:70` 非目标条款确认为产品定位主动排除，判断更清晰但外部视角能力缺失结论不变。824cde9 新增的 retry/refresh/compact/checkpoint/websearch/trust/approval/workflow_journal 经 grep 核实均不触及 mcp/lsp/plugin/sdk，不影响本维度状态判定。

---

## 5. 差距总清单（P0/P1/P2 排序）

### P0

| #    | 差距              | kxen 证据                   | 竞品基线     | 状态标记 |
| ---- | ----------------- | --------------------------- | ------------ | -------- |
| P0-1 | OS 级沙箱完全空白 | grep `sandbox-exec          | Seatbelt     | bwrap    | seccomp` 零命中 | Claude/Codex/Gemini 均有 | 仍存在 |
| P0-2 | MCP 完全缺失      | grep `mcp` 零命中；仅设计稿 | 9 家全部支持 | 仍存在   |

### P1

| #     | 差距                               | kxen 证据                                             | 竞品基线                              | 状态标记           |
| ----- | ---------------------------------- | ----------------------------------------------------- | ------------------------------------- | ------------------ |
| P1-1  | 项目级 hooks 信任门死代码          | `config.rs:112-118`；9 处 Config::load 无一传项目路径 | Codex/Gemini Trusted Folders          | 已收窄             |
| P1-2  | 无后台持续记忆 consolidation       | `distill.rs:56-87` 仅会话删除触发                     | Codex Memories/Claude Auto memory     | 仍存在             |
| P1-3  | Goal::focus() 进程级全局单例       | `goal.rs:205-209`；`run.rs:218`                       | Codex `/goal` thread-scoped           | 新识别             |
| P1-4  | MRM 与运行时重试换账号两套独立机制 | `run.rs` grep `mrm.` 零命中                           | Gemini ModelAvailabilityService       | 已收窄             |
| P1-5  | 审批仅单档，未达竞品多档           | `rules.rs:8-29` 仅 1 档 Ask                           | Claude 七档/Codex 三档 x 三档         | 已收窄             |
| P1-6  | doctor 自检仅凭据一维              | `doctor.rs:24-60`                                     | claude doctor 9 类以上                | 仍存在             |
| P1-7  | 无并行 workspace 中心看板          | `RightColumn.tsx:7,76` 侧边单列                       | Conductor/Vibe Kanban/Cline           | 仍存在             |
| P1-8  | 网络隔离/域名 allowlist 缺失       | webfetch/websearch 直连                               | Codex/Claude/Gemini 均有              | 仍存在（略扩大）   |
| P1-9  | 内置 provider 广度仅 4 家          | `client.rs:45-81`                                     | OpenCode 75+/Crush 30+                | 仍存在             |
| P1-10 | 无非流式/结构化输出                | `anthropic.rs:205`                                    | Codex SDK/Gemini output_schema        | 仍存在             |
| P1-11 | LSP/代码智能缺失                   | grep `lsp` 零命中                                     | OpenCode/Crush                        | 仍存在             |
| P1-12 | 无 SDK/编程接口                    | `src-tauri/src/bin` 不存在                            | Claude/Codex/OpenCode/Cline           | 仍存在             |
| P1-13 | 无 headless/CI                     | `main.rs` 无 args                                     | Claude/OpenCode/Goose                 | 仍存在（定位排除） |
| P1-14 | 无插件/marketplace                 | `prd.md:70` 非目标                                    | Claude/Gemini/Codex/Cline             | 仍存在（定位排除） |
| P1-15 | 第三方规则互操作仅根级单文件       | `scan.rs:16-27`                                       | Crush 全兼容/Cline 目录格式           | 已收窄             |
| P1-16 | web 检索深度落后                   | `websearch.rs`（DuckDuckGo HTML）                     | Crush sourcegraph/OpenCode codesearch | 已收窄             |

### P2

| #     | 差距                                         | kxen 证据                               | 状态标记 |
| ----- | -------------------------------------------- | --------------------------------------- | -------- |
| P2-1  | subagent 禁止派孙代理                        | `subagent.rs:152`；`execute.rs:248-249` | 新识别   |
| P2-2  | subagent 角色自定义无 per-role model/effort  | `subagent.rs:96-127`                    | 已收窄   |
| P2-3  | workflow resume 仅派发级缓存非完整重放       | `workflow_journal.rs`                   | 已收窄   |
| P2-4  | 持久后台会话/daemon 未见改动                 | cron 已持久，daemon 无                  | 已收窄   |
| P2-5  | 活动注册表纯内存                             | `activity.rs:1-9`                       | 仍存在   |
| P2-6  | checkpoint 无分档模式/预览/fork 联动         | `session_ops.rs:17-30`                  | 已收窄   |
| P2-7  | 记忆检索无 tag/usage/语义维度                | `render.rs:10-28`                       | 已收窄   |
| P2-8  | 无组织/managed policy 强制层                 | 仅双 scope                              | 仍存在   |
| P2-9  | rules/reference 无 @import                   | 未见机制                                | 仍存在   |
| P2-10 | loop 检测器各自独立不聚合                    | `loop_detect.rs:49` 多处实例化          | 仍存在   |
| P2-11 | doctor 刷新文案与失败场景缝隙                | `doctor.rs:29,42`                       | 已收窄   |
| P2-12 | 用户可配置 allow 规则语法缺失                | `rules.rs` 硬编码                       | 仍存在   |
| P2-13 | Fish 快照路径硬编码单一架构                  | `shell.rs:22`（Apple Silicon 路径）     | 已收窄   |
| P2-14 | composer 附件/diff 回传/分享链接/UI 内 PR 缺 | 本次未触及                              | 仍存在   |
| P2-15 | 重试上限/退避封顶偏保守                      | `retry.rs` MAX_ATTEMPTS=3               | 新识别   |
| P2-16 | 公网图片 URL 输入缺失                        | `types.rs:30-40`                        | 仍存在   |
| P2-17 | 非开源/无 LICENSE/仅单平台                   | 无 LICENSE；`package.json:3` private    | 仍存在   |
| P2-18 | 输出截断无落盘/后台任务不持久                | 不在 A-G 范围                           | 仍存在   |
| P2-19 | hooks 单一 shell handler（生态口径）         | `hooks.rs`                              | 仍存在   |

已关闭（不再列为差距）：无请求重试（P0）、无 OAuth 主动刷新（P0）、无上下文压缩（P0/P1）、goal record_turn 未接线（P0）、无 ask-user 审批档（P0->建立）、命令解析绕过面（P1）、进程组 kill 孙进程泄漏（P1）、subagent 不可自定义基础能力（P1）、workflow 无 resume 基础能力（P1）、Team 不跨进程（P2）、cron 纯内存（P2）、GoalUpdate 未 publish（P2）、injection_preview 固定空 involved（P2）、SessionTree 不可展开（P2）、RoutingSection fallback 只读（P2）、UsageSection stub（P2）、后台 OS 推送缺口（P1）、SessionTree 展开（P2）。

---

## 6. kxen 独有优势清单

以下为在 10 家中 kxen 明确领先或独有的能力（附证据）：

1. MRM 全局资源调度器（10 家唯一）：角色路由 + 降级链 + per-provider/全局 semaphore + RPM 滑窗 + 多账号轮转一体（`mrm.rs:48-225`）。本次未稀释。
2. 重试与账号轮转一体设计（补全后新增差异化）：`retry.rs` 把请求重试与账号轮转耦合为同一容错路径，接线 `run.rs:97-164`，竞品未见等价公开实现。
3. LLM 请求四位一体运行时韧性层（重试 + 退避 + 账号轮换 + OAuth 主动刷新全接主循环）：唯一有完整记录的四合一韧性实现（`retry.rs` + `refresh.rs` + `run.rs:95,97-164`）。
4. 四层递进 loop 检测（覆盖面最完整）：exact/semantic/stagnation/churn 接入 4 类运行路径（`loop_detect.rs:13-122`）。
5. QuickJS 沙箱 workflow 补齐 resume（唯二具备 resume 的原生脚本编排引擎之一）：`workflow.rs:19-179` + `workflow_journal.rs`。
6. OKF 单规范统一 7 类知识 + 引擎级四态分级注入 + 注入级信任分级：`render.rs` + `trust.rs:41-53`，无对标竞品在知识注入层有等价信任分级。
7. shadow-git checkpoint 默认自动打点 + 代码/对话双维度真回滚同一操作内完成：`checkpoint.rs:1-114` + `session_ops.rs:17-30`。
8. 上下文自动压缩带降级兜底（不假装蒸馏出内容，明确标注 fallback）：`compact.rs:1-118`。
9. Goal 状态机形式化完整度反超 Codex 且已消除接线缺口（`goal.rs:5-174` + `run.rs:218-246`）。
10. 会话时间线原生嵌入交互式审批（Verdict::Ask ApprovalCard），同一 broker 复用于项目信任门（`approval.rs:1-50` + `ApprovalCard.tsx`）。
11. glob 条件激活 + mid-turn 系统提示刷新联动（`render.rs:54-60` + `run.rs:45-47`）。
12. 会话删除兜底蒸馏（独特触发时机，`distill.rs:56-87`）。
13. auto_bg 15s 自动前台转后台（`exec.rs:14`）；rm->trash 透明遮蔽 + trash 语义删除全链路（`shell.rs:85-89` + `fs_tool.rs:221-246`）。
14. F1-F5 破坏命令语义分类器 + 解析绕过面已封闭（`eval.rs:12-169`），无沙箱前提字符串层防护达理论上限。
15. hashline 锚点编辑 + find_shifted 自愈（`fs_tool.rs:68-207`）。
16. 语音本地流式识别 + 多引擎混合（`voice/mod.rs`）；角色只读权限编译期单测强约束（`subagent.rs:174-184`）；自研 agent + 原生 Tauri GUI 一体。

注意：优势 13-15 属能力独有性判断；因缺 OS 沙箱 + 多档审批 + 项目 hooks 信任门（P0/P1），kxen 整体安全模型仍弱于 Claude Code/Codex/Gemini。多订阅寄生官方 CLI 登录态存在实质 ToS 风险，非可持续设计优势（详见 4.1 风险提示）。

---

## 7. 相对 2026-07-23 旧报告的变更摘要

### 7.1 补全计划 A-G 关闭的旧 P0/P1（均接线真实运行路径）

- authllm：请求重试（P0 closed，`retry.rs` + `run.rs:97-164`）、OAuth 主动刷新（P0 closed，`refresh.rs` + `run.rs:95`）、doctor 文案名实相符（closed）。
- agent：goal record_turn 接线主循环（P0 closed，`run.rs:218-246`）、GoalUpdate 双路径 publish（closed）、subagent 角色自定义基础能力（P1 closed）、workflow resume 基础能力（P1 closed）、Team 跨进程 restore（P2 closed）、cron 持久化（P2 closed）。
- tools：Verdict::Ask 审批全链路（P0 建立，收窄 P1）、命令解析绕过面（P1 closed）、进程组 killpg（P1 closed）、hooks 六类事件（closed）、.agents 知识注入信任门（closed）。
- knowledge：injection_preview 真实 involved（P2 closed）、第三方规则互操作扩 4 种（P1 narrowed）、记忆 top-K 检索（P1 narrowed）。
- ui：checkpoint/rewind（P1 收窄 P2）、后台 OS 推送（P1 基本关闭）、SessionTree 展开（P2 closed）、UsageSection/RoutingSection（closed）。
- 可靠性：上下文压缩（P0/P1 closed，`compact.rs`）、Goal 状态机接线（P0 closed）。

### 7.2 新增能力（旧报告未提及）

- checkpoint/rewind shadow git 真实时间旅行（`checkpoint.rs`，代码 + 会话双回滚）。
- websearch 真实工具（`websearch.rs`，DuckDuckGo HTML）。
- core/trust.rs 项目信任门接入知识注入侧（未信任 project scope 只索引不全文注入）。
- DockWorktree 脏文件计数 + 一键切换；OS 桌面推送非前台会话通知。

### 7.3 仍缺（本次未动）

- P0：OS 级沙箱、MCP。
- P1：项目级 hooks 信任门死代码、无后台持续记忆 consolidation、doctor 单维度、并行 workspace 中心看板、网络隔离、provider 广度、非流式/结构化输出、LSP、SDK、headless/CI、插件市场（后三项为 `prd.md:70` 主动排除）。
- 生态维度自基线以来核心结论未变，除 web 检索收窄外无实质变化。

### 7.4 新识别差距（代码核对后暴露）

- P1 Goal::focus() 进程级全局单例，多 session 并发预算/阻塞互相误伤（`goal.rs:205-209` + `run.rs:218`）：接线修复真实，但接线后暴露语义粒度缺陷，旧文档因未接线不可观测。
- P2 subagent 结构性禁止派孙代理（`subagent.rs:152`）：设计层硬限制，多数竞品对深度嵌套亦谨慎。
- P2 重试上限/退避封顶偏保守（`retry.rs` MAX_ATTEMPTS=3）：旧基线无重试，落地后才可对比。

### 7.5 结论

补全计划 A-G 是实质性正向变化：旧报告标注的多条 P0/P1（重试、刷新、压缩、goal 接线、审批、解析绕过、进程组 kill、Team/cron 持久化）均已真实闭合并进入运行主链路，不是装饰。kxen 的自治可靠性与请求韧性从「设计完整但运行路径大面积未接线」跨越到「核心防失控闭环真实生效」。但两块根本代差未动：一是安全模型（OS 沙箱、网络隔离、多档审批、项目 hooks 信任门），二是生态开放性（MCP/LSP/SDK/headless/插件市场，其中后三项为产品定位主动排除）。当前最紧迫的三项：OS 沙箱（P0）、MCP（P0）、以及接线后新暴露的 Goal 进程级全局单例语义缺陷（P1）。
