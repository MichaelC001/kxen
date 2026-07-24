# kxen 当前实现与竞品差异评估报告

- 报告日期：2026-07-23
- 代码基线：`9484e43c2b94cca3f5c01f2562fcdcbdf5a8a092`
- 分支基线：`main` 与 `origin/main` 同步
- 仓库：[kxen](file:///Users/xiaobai/Code/SelfCode/kxen)
- 代码规模：311 个 tracked 文件，约 15433 行 Rust，约 7502 行 TypeScript 和 TSX
- 对比产品：Claude Code、OpenAI Codex、Gemini CLI、OpenCode、Goose、Cursor、Windsurf、Cline、Roo Code、Conductor、Vibe Kanban
- 评估范围：功能细节、交互细节、运行主链、持久化、并发隔离、恢复、安全、生态、性能、可观测性、发布成熟度

## 0. 最终结论

### 0.1 结论摘要

kxen 已经具备完整桌面 coding agent 的主要功能面，不是原型界面。当前代码包含多 provider 认证、流式 agent loop、文件和进程工具、会话持久化、checkpoint、worktree、goal、subagent、team、QuickJS workflow、cron、MCP、LSP diagnostics、知识注入、自动记忆蒸馏、语音输入、通知和用量视图。

当前实现不能判定为生产级完成。决定性原因不是功能数量，而是安全边界、会话隔离、并发一致性和恢复语义没有闭合。最高风险路径包括：

- 本地 WebSocket 无认证和 Origin 校验，并向每个连接直接发送所有带 `stream_id` 的 LLM 流。
- cron 可以绕过前台 active run 保护，为同一 session 启动并发 run，而 JSONL 写入没有 session 锁。
- `exec` 使用一个目录做安全判断，再使用另一个目录执行命令。
- `task start`、worktree 删除和部分 hook 路径绕过统一审批语义。
- 文件上下文和 URL 上下文可以绕过路径守卫或访问私网地址。
- MCP 工具在角色权限过滤之后追加，readonly 和 plan-only agent 仍能获得 MCP 写工具。
- rewind 会对整个工作目录执行 `git reset --hard`，没有 active run、dirty tree、跨 session 和用户编辑保护。
- 全局 `SessionExtras`、全局 model 和启动时冻结的 `TeamManager` 依赖破坏 session 和 workspace 隔离。
- 主会话和 team LLM 请求没有进入 MRM 的并发和 RPM 占槽路径。

### 0.2 分级结果

| 维度             | 等级 | 结论                                                                                           |
| ---------------- | ---: | ---------------------------------------------------------------------------------------------- |
| 功能广度         |    A | 核心 coding agent、桌面交互、多 agent、知识、语音和扩展面均有实现                              |
| 桌面交互细节     |    B | 时间线、composer、Dock、设置页完整，但附件、草稿、重连和 Markdown 渲染有正确性缺口             |
| Agent 自治与编排 |    B | goal、subagent、team、workflow、cron 都已接线，但权限、恢复、限流和失败状态不完整              |
| 持久化与恢复     |    C | session、team、schedule、journal、checkpoint 均持久化，但恢复不是事务级，部分上下文会丢失      |
| 并发与隔离       |    D | model、extras、team dependencies、notifications 和 usage 为进程级状态，同 session 并发写已确认 |
| 安全边界         |    D | 无 OS sandbox、无网络隔离、本地 RPC 无认证，且存在多条审批和路径边界绕过                       |
| 生态与代码智能   |    C | MCP stdio 和 Rust diagnostics 可用，远端 MCP、多语言 LSP、SDK、headless 均缺失                 |
| 可靠性与可观测性 |    C | 有 retry、loop detection、cancel、usage 和 notifications，但统计、bus lag、恢复和门禁不闭合    |
| 发布成熟度       | FAIL | 当前 Rust 格式门禁和前端测试命令失败，缺少 updater 和签名配置，未完成真实 App E2E              |

### 0.3 相对同类软件的位置

- 相对 Claude Code、Codex 和 Gemini CLI：kxen 的桌面整合和多 provider 凭证复用更集中，但 sandbox、权限系统、MCP 完整度、headless 和生产可靠性明显落后。
- 相对 OpenCode 和 Goose：kxen 的 goal、team、workflow、voice 和原生桌面面板更完整，但 provider 广度、插件或 extension 生态、LSP 和远端 MCP 落后。
- 相对 Cursor、Windsurf、Cline 和 Roo Code：kxen 的自建 runtime 和多账号调度更强，但 IDE code intelligence、browser agent、细粒度 checkpoint、worktree 并行体验和权限成熟度落后。
- 相对 Conductor 和 Vibe Kanban：kxen 不只是 harness 外壳，拥有自己的 agent runtime；但 workspace、worktree、任务、diff 和 review 尚未形成第一等并行工作单元。

## 1. 评估口径

### 1.1 状态定义

- PASS：当前源码存在完整主链调用点。PASS 不代表真实 App 已运行验收。
- PARTIAL：存在实现和调用点，但行为不完整、存在已确认错误，或只覆盖能力子集。
- FAIL：实现或门禁与产品声明直接冲突。
- UNKNOWN：未执行真实 provider、真实 App、真实 MCP、真实 LSP 或真实系统权限流程，不能宣称运行通过。

### 1.2 证据层级

- STATIC：逐文件阅读源码并追踪定义、wiring、调用和持久化。
- TESTED：本轮实际运行命令并记录退出码。
- LIVE：真实桌面 App、provider、MCP、LSP、语音和系统权限验收。本轮全部为 UNKNOWN。

### 1.3 竞品资料口径

- 产品功能资料通过 Exa 检索，并使用产品官方文档或官方站点。
- 开源实现细节通过 gh_grep 检索，并引用对应 GitHub 源码。
- 竞品未被当前官方资料直接支持的能力标记 UNKNOWN，不用历史印象补全。
- Vibe Kanban 当前官方站点显示项目进入 sunsetting，本报告仍评估其开源 workspace 和 agent orchestration 设计。

## 2. 当前架构与运行主链

### 2.1 运行结构

```text
SolidJS UI
  -> Tauri command 获取随机本地端口
  -> ws://127.0.0.1:{random_port} WebSocket JSON-RPC
  -> RPC dispatcher
  -> run_llm
  -> agent loop
  -> provider stream 或 tool execution
  -> EventBus
  -> WebSocket stream
  -> UI timeline 和 Dock
```

核心证据：

- 单一全局状态容纳 model、MRM、extras、MCP、LSP、hooks、team、active runs、pending queue 和 UI 统计：[main.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/main.rs#L9-L105)
- 本地随机端口 WebSocket server：[ws/mod.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/ws/mod.rs#L48-L88)
- 主 run 从 session 目录恢复历史并构造 agent context：[llm_task.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/ws/llm_task.rs#L10-L270)
- 每轮 LLM stream、retry、tool accumulation 和 tool execution：[run.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/agent_loop/run.rs#L44-L239)
- 会话采用 metadata JSON 加 message JSONL：[session.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/core/session.rs#L1-L200)

### 2.2 持久化边界

| 数据                                                  | 当前介质             | 恢复语义                                           | 结论                                                |
| ----------------------------------------------------- | -------------------- | -------------------------------------------------- | --------------------------------------------------- |
| Session metadata                                      | JSON                 | 可恢复 title、directory、pin、sort、parent         | PASS                                                |
| Session messages                                      | JSONL                | 可恢复 text、context、tool summary、reasoning type | PARTIAL，image 和 streamed reasoning 实际未完整落盘 |
| Team config 和 tasks                                  | JSON                 | 重启后成员统一降为 Shutdown                        | PARTIAL                                             |
| Team inbox                                            | append JSON lines    | 读后整体清空                                       | PARTIAL，存在并发丢消息窗口                         |
| Schedule                                              | JSON                 | App 启动后恢复并由 15 秒 tick 触发                 | PARTIAL，无跨进程 scheduler                         |
| Workflow journal                                      | JSONL                | 相同 run id 和 role/prompt 命中旧结果              | PARTIAL，无脚本身份和过期策略                       |
| Checkpoint                                            | 私有 bare Git        | 按 message id 找 commit 并 hard reset              | PARTIAL，恢复粒度过大                               |
| Knowledge 和 memory                                   | Markdown 加水位 JSON | 可注入后续 prompt                                  | PARTIAL，失败也可推进水位                           |
| Notifications、draft、pending queue、agent transcript | 内存                 | 重启丢失                                           | FAIL                                                |

### 2.3 根本架构问题

AppState 同时承担全局服务、workspace 状态、session 状态和 UI cache。当前没有明确的 `WorkspaceRuntime` 和 `SessionRuntime` 生命周期边界。结果是：

- session 级 extras 实际为进程级共享。
- model 是进程级单值，不随 session 持久化。
- TeamManager 在启动时捕获 workdir、auth、LSP 和 extras，workspace switch 只更新 active workspace、hooks 和 AppState 中的 LSP。
- session 的目录来自 metadata，但 team 仍使用启动目录。
- 多条内存状态在 session 删除、workspace 切换和 App 重启时没有统一清理。

## 3. 当前实现的全部功能细节

## 3.1 桌面壳、workspace 和 session

| 功能               | 当前细节                                                                     |    状态 | 证据                                                                                               |
| ------------------ | ---------------------------------------------------------------------------- | ------: | -------------------------------------------------------------------------------------------------- |
| Tauri 桌面壳       | Tauri 2 加 SolidJS，macOS 最低版本 14，bundle 目标为 dmg                     |    PASS | [tauri.conf.json](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/tauri.conf.json#L1-L39)       |
| 本地 RPC           | 单 WebSocket 端点承载 request、response、server stream 和 topic subscription |    PASS | [ws/mod.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/ws/mod.rs#L1-L156)              |
| Workspace 注册     | 可 add、list、touch、overview 和 switch                                      |    PASS | [workspace.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/core/workspace.rs#L1-L100)   |
| Workspace 首页     | 最近 workspace 网格，可切换进入 session 页面                                 |    PASS | [Workspaces.tsx](file:///Users/xiaobai/Code/SelfCode/kxen/src/pages/Workspaces.tsx#L1-L89)         |
| Workspace 并行模型 | active workspace 仍为进程级单值，网格不是同时运行的 workspace board          | PARTIAL | [main.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/main.rs#L45-L47)                  |
| Session 创建       | directory 默认取 active workspace，metadata 持久化                           |    PASS | [rpc.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/ws/rpc.rs#L88-L100)                |
| Session 列表       | 按 directory 分组，支持 rename、pin、手动排序和删除                          |    PASS | [SessionTree.tsx](file:///Users/xiaobai/Code/SelfCode/kxen/src/components/SessionTree.tsx#L1-L236) |
| Session fork       | 复制目标消息之前的历史并记录 parent id                                       | PARTIAL | [session.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/core/session.rs#L186-L203)     |
| Session export     | 导出 Markdown，保留正文和 tool summary，省略 reasoning 和 context            |    PASS | [session.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/core/session.rs#L205-L263)     |
| Session 删除       | metadata 和 JSONL 移入 Trash，删除前调用 distill                             | PARTIAL | [rpc.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/ws/rpc.rs#L102-L137)               |
| 运行中发送         | 设计支持 queue 或 interrupt                                                  |    FAIL | [config.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/core/config.rs#L8-L18)          |
| Pending queue      | 同 session run 结束后自动续跑下一条                                          | PARTIAL | [llm_task.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/ws/llm_task.rs#L309-L330)     |
| Foreground session | 用于决定 OS notification 是否弹出                                            |    PASS | [main.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/main.rs#L55-L57)                  |
| Session 删除清理   | 不取消 active run，不删除 schedule、goal、team、snapshot 和 pending queue    |    FAIL | [rpc.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/ws/rpc.rs#L102-L137)               |

关键差异：

- Conductor、Vibe Kanban、Cursor 和 Windsurf Arena 把 workspace 或 worktree 作为并行任务的第一等运行单元。kxen 的 workspace 主要是导航和 active directory 选择。
- kxen session metadata 不保存 model、account、permission profile、workspace runtime version 和 checkpoint head，恢复精度低于成熟 agent session。

## 3.2 对话时间线和 composer

| 功能             | 当前细节                                            |    状态 | 证据                                                                                                            |
| ---------------- | --------------------------------------------------- | ------: | --------------------------------------------------------------------------------------------------------------- |
| 流式文本         | 50ms delta batching，Done 后以持久化快照对账        |    PASS | [delta-batch.ts](file:///Users/xiaobai/Code/SelfCode/kxen/src/lib/delta-batch.ts#L1-L31)                        |
| Reasoning 展示   | stream 时展示 reasoning                             | PARTIAL | [Session.tsx](file:///Users/xiaobai/Code/SelfCode/kxen/src/pages/Session.tsx#L135-L170)                         |
| Tool timeline    | 展示 tool call、tool result 和摘要                  |    PASS | [AssistantItem.tsx](file:///Users/xiaobai/Code/SelfCode/kxen/src/components/AssistantItem.tsx#L1-L94)           |
| Approval card    | 高危 exec 等待用户 allow 或 deny                    |    PASS | [ApprovalCard.tsx](file:///Users/xiaobai/Code/SelfCode/kxen/src/components/ApprovalCard.tsx#L1-L38)             |
| Markdown         | marked、Shiki、Mermaid、copy code                   | PARTIAL | [markdown.ts](file:///Users/xiaobai/Code/SelfCode/kxen/src/lib/markdown.ts#L1-L108)                             |
| Message actions  | copy、rerun、edit and resend、rewind                |    PASS | [MessageActions.tsx](file:///Users/xiaobai/Code/SelfCode/kxen/src/components/MessageActions.tsx#L1-L80)         |
| Composer trigger | `@` context、`/` command、`#` knowledge             |    PASS | [triggers.ts](file:///Users/xiaobai/Code/SelfCode/kxen/src/components/composer/triggers.ts#L1-L104)             |
| Large paste      | 大文本折叠为 paste chip                             |    PASS | [paste.ts](file:///Users/xiaobai/Code/SelfCode/kxen/src/components/composer/paste.ts#L1-L46)                    |
| Image input      | 本轮发送支持 base64 image parts                     | PARTIAL | [TextComposer.tsx](file:///Users/xiaobai/Code/SelfCode/kxen/src/components/composer/TextComposer.tsx#L240-L267) |
| 普通文件附件     | 前端只保留 file name，后端按 workspace 相对路径读取 |    FAIL | [TextComposer.tsx](file:///Users/xiaobai/Code/SelfCode/kxen/src/components/composer/TextComposer.tsx#L205-L240) |
| 每 session draft | 使用组件内 Map 按 active session id 保存            | PARTIAL | [TextComposer.tsx](file:///Users/xiaobai/Code/SelfCode/kxen/src/components/composer/TextComposer.tsx#L40-L106)  |
| IME 防误发       | composition end 后设置短锁窗                        |    PASS | [TextComposer.tsx](file:///Users/xiaobai/Code/SelfCode/kxen/src/components/composer/TextComposer.tsx#L40-L44)   |
| Command palette  | 支持 session、command 和 knowledge 搜索             |    PASS | [CommandPalette.tsx](file:///Users/xiaobai/Code/SelfCode/kxen/src/components/CommandPalette.tsx#L1-L154)        |
| Scroll pin       | 用户上翻后停止自动滚动并提供回到底部                |    PASS | [Session.tsx](file:///Users/xiaobai/Code/SelfCode/kxen/src/pages/Session.tsx#L50-L69)                           |

已确认细节错误：

- `marked.parse` 原样保留 raw HTML，组件随后写入 `innerHTML`，没有 sanitizer。Mermaid 的 `securityLevel` 只保护 Mermaid，不保护普通 Markdown HTML。[Markdown.tsx](file:///Users/xiaobai/Code/SelfCode/kxen/src/components/Markdown.tsx#L20-L29)
- 新 session 首发时，draft 先记录在空 id 下，发送后 active id 改变，删除操作针对新 id，空 id draft 会残留并在下一次新 session 恢复。[TextComposer.tsx](file:///Users/xiaobai/Code/SelfCode/kxen/src/components/composer/TextComposer.tsx#L100-L106)
- image 只附在当前 request，不进入 session `Part`。重启、fork、export 和后续历史重放均失去 image。[session.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/core/session.rs#L23-L41)
- stream reasoning 没有加入 transcript writer，reload 后消失。[llm_task.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/ws/llm_task.rs#L240-L270)

## 3.3 Provider、认证、model 和用量

| 功能             | 当前细节                                                                              |    状态 | 证据                                                                                               |
| ---------------- | ------------------------------------------------------------------------------------- | ------: | -------------------------------------------------------------------------------------------------- |
| 内置 provider    | Anthropic、OpenAI、xAI、Kimi、OpenRouter、Ollama                                      |    PASS | [client.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/llm/client.rs#L1-L100)          |
| Custom provider  | 支持 OpenAI 或 Anthropic 兼容协议、自定义 base URL 和 model list                      |    PASS | [config.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/core/config.rs#L20-L34)         |
| 凭证探测         | 从 Claude、Codex、Grok 和 Kimi CLI 相关存储读取凭证                                   |    PASS | [probe.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/auth/probe.rs#L26-L108)          |
| 多账号           | credential store 支持 provider account key                                            |    PASS | [credential.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/auth/credential.rs#L1-L111) |
| OAuth refresh    | Anthropic 和 OpenAI 可主动刷新                                                        | PARTIAL | [refresh.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/auth/refresh.rs#L18-L100)      |
| Provider verify  | 发起最小真实请求验证配置                                                              |    PASS | [verify.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/llm/verify.rs#L1-L59)           |
| Model catalog    | 本地静态 catalog 加远端 model list                                                    |    PASS | [catalog.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/llm/catalog.rs#L1-L210)        |
| Vision           | 多 provider 适配 image parts                                                          |    PASS | [types.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/llm/types.rs#L1-L50)             |
| Retry            | 最多 3 次，只在尚无任何 delta 时重试，指数退避并轮换账号                              |    PASS | [retry.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/llm/retry.rs#L1-L60)             |
| MRM role routing | 按 role、provider、model、fallback 和 account 选择模型                                |    PASS | [mrm.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/llm/mrm.rs#L1-L180)                |
| MRM concurrency  | semaphore 加 provider RPM window                                                      | PARTIAL | [mrm.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/llm/mrm.rs#L168-L225)              |
| Session model    | model 为 AppState 全局单值，session metadata 不持久化                                 |    FAIL | [main.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/main.rs#L9-L17)                   |
| Model controls   | 没有 temperature、max output、reasoning effort、structured output 等 session controls |    FAIL | [types.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/llm/types.rs#L1-L80)             |
| Usage            | 保存最近一次 AgentOutcome stats 并累计到内存                                          | PARTIAL | [llm_task.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/ws/llm_task.rs#L283-L300)     |

已确认细节错误：

- MRM `acquire` 只在 subagent dispatch 使用。主 session 和 team member 的真实 LLM 请求没有持有 provider 或 global slot。[subagent.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/subagent.rs#L132-L160)
- `resolve` 和 `acquire` 分开执行，选择结果和 slot 获取不是原子过程。
- provider semaphore 使用 account slot key，实际并发限制按账号拆开，不能形成 provider 总上限。
- usage 变量被每次 Delta::Usage 覆盖，只保留最后一个 Usage delta，不做跨 request 累计。多轮 tool loop 会低估总 token。
- goal 的 record turn 发生在 tool round 后，最终无 tool 的 response 不进入 goal usage。
- retry 不解析 `Retry-After`。
- 凭证会复制到 kxen auth JSON，文件权限为 0600，但仍扩大了明文凭证副本和生命周期。

相对竞品：

- OpenCode 和 Goose 的 provider 覆盖更广。
- Codex、Claude Code、Cursor 和 Windsurf 采用各自受控 model surface，provider 广度较窄，但 session、cloud task 和权限边界更一致。
- kxen 的四类 CLI 凭证复用和多账号池是明确差异化，但当前 MRM 没有覆盖最高频主链。

## 3.4 Agent loop、goal、subagent、team、workflow 和 schedule

| 功能                | 当前细节                                                             |    状态 | 证据                                                                                                           |
| ------------------- | -------------------------------------------------------------------- | ------: | -------------------------------------------------------------------------------------------------------------- |
| Agent loop          | 最多 32 turn，stream text、reasoning、tool fragments、usage 和 error |    PASS | [run.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/agent_loop/run.rs#L44-L239)              |
| Tool loop           | 模型返回多个 tool call 时逐个顺序执行                                | PARTIAL | [run.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/agent_loop/run.rs#L180-L225)             |
| Tool disclosure     | resident tools 加 deferred todo、webfetch 和 websearch               |    PASS | [tools_spec.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/tools_spec.rs#L1-L326)            |
| Todo                | add、complete、clear done 和 list                                    | PARTIAL | [todo.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/todo.rs#L1-L77)                         |
| Session extras      | deferred tools、todo、loaded skills 和 recursion depth               |    FAIL | [context.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/agent_loop/context.rs#L11-L20)       |
| Loop detection      | exact、semantic、stagnation 和 ABABAB 检测                           |    PASS | [loop_detect.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/loop_detect.rs#L1-L140)          |
| Cancel              | session abort 和 stream cancel 路由到 CancelToken                    |    PASS | [cancel.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/cancel.rs#L1-L40)                     |
| Goal                | 8 状态、token、turn、wall budget、连续相同 blocker 计数              |    PASS | [goal.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/core/goal.rs#L1-L222)                         |
| Goal session scope  | 主 loop 使用 session focus，状态栏仍使用全局 focus                   | PARTIAL | [settings.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/ws/settings.rs#L28-L36)                   |
| Goal evidence       | complete 仅检查 evidence 非空，不校验测试、文件或外部结果            | PARTIAL | [goal.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/core/goal.rs#L120-L180)                       |
| Subagent            | thinking、planning、execution、review、research 和 custom role       |    PASS | [subagent.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/subagent.rs#L80-L125)               |
| Subagent permission | readonly profile 限制 resident tools                                 | PARTIAL | [subagent.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/subagent.rs#L48-L75)                |
| Subagent nesting    | child context 的 MRM 为 None，不能继续 dispatch agent                |    PASS | [subagent.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/subagent.rs#L145-L180)              |
| Team                | spawn、message、shutdown、plan approval、task 和依赖                 |    PASS | [manager.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/team/manager.rs#L1-L250)             |
| Team member loop    | 常驻 loop，idle 等 inbox notify，再重建本轮 messages                 | PARTIAL | [member_loop.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/team/member_loop.rs#L15-L100)    |
| Team persistence    | config、members、tasks 和 inbox 持久化                               | PARTIAL | [types.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/team/types.rs#L15-L80)                 |
| Agent activity      | UI 可查看 agent status 和 transcript，内存 capped                    |    PASS | [activity.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/activity.rs#L1-L110)                |
| Workflow            | QuickJS 运行 `agent`、`phase`、`log` 和 constraints                  |    PASS | [workflow.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/workflow.rs#L1-L220)                |
| Workflow limits     | 64MB memory、1MB stack、10 分钟、最多 32 agents                      |    PASS | [workflow.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/workflow.rs#L80-L130)               |
| Workflow resume     | journal 按 run id 和 role/prompt hash 复用结果                       | PARTIAL | [workflow_journal.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/workflow_journal.rs#L1-L62) |
| Schedule            | 持久化 cron 或 once job，每 15 秒触发 session run                    | PARTIAL | [schedule.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/core/schedule.rs#L1-L136)                 |

已确认细节错误：

- `research` 的 system prompt 要求 external research，但 readonly allowed tools 不包含 deferred web tools。
- `planning` 使用 ReadonlyTodo，但具体 allowed tools 和 deferred todo 挂载顺序不一致，不能保证 todo 可用。
- custom role name 未验证，直接拼入项目 role 文件路径，并且 custom role 文件没有独立 trust gate。
- subagent 结束时即使内部 error 或 abort，也会把 `final_text` 作为成功结果返回 parent。
- TeamManager 在 App 启动时捕获原 workdir、auth store、extras 和 LSP。workspace switch 不更新这些依赖。[main.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/main.rs#L72-L99)
- team member 每次 wake 只用原始任务加最新 inbox 重建 messages，没有完整前轮历史。
- team task 只有 Pending、InProgress、Completed，没有 Failed、Canceled 和 Reassigned。
- 重启 restore 将活跃成员设为 Shutdown，但 cancels 和 notifies 为空，同名成员不能直接恢复原 loop。
- teammate idle hook 拒绝时只 append inbox，没有触发 notify，member 随后可永久等待。
- inbox 的 read 加整体 truncate 没有和 append 共享锁，存在读取后、清空前的新消息丢失窗口。[inbox.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/team/inbox.rs#L18-L38)
- workflow `run_id` 未验证并进入文件名，journal 使用 `DefaultHasher`，没有脚本版本、输入版本、过期和清理策略。
- cron 直接 spawn `run_llm`，没有检查同 session 是否已运行。[main.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/main.rs#L214-L241)

## 3.5 文件、进程、搜索、Web、dev server、hooks 和审批

| 功能            | 当前细节                                                                                |    状态 | 证据                                                                                                       |
| --------------- | --------------------------------------------------------------------------------------- | ------: | ---------------------------------------------------------------------------------------------------------- |
| Read            | 文本读取，最多前 2000 行                                                                | PARTIAL | [fs_tool.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/fs_tool.rs#L1-L108)              |
| Edit            | 支持 exact match 和 hashline anchors，检查 file freshness                               |    PASS | [fs_tool.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/fs_tool.rs#L109-L220)            |
| Write           | 写前 snapshot，路径守卫                                                                 |    PASS | [fs_tool.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/fs_tool.rs#L221-L260)            |
| Delete          | 使用 Trash，不直接硬删                                                                  |    PASS | [fs_tool.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/fs_tool.rs#L221-L260)            |
| Glob 和 grep    | 支持 workspace 搜索并设结果 cap                                                         | PARTIAL | [search.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/search.rs#L1-L140)                |
| Exec            | zsh、bash、fish 方言校验和 shell snapshot                                               |    PASS | [shell.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/shell.rs#L1-L100)                  |
| Exec safety     | F1 到 F5 destructive rules、nested shell 和 command substitution 解析                   | PARTIAL | [eval.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/safety/eval.rs#L1-L276)             |
| Exec approval   | Verdict Ask 进入 ApprovalBroker                                                         |    PASS | [exec.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/exec.rs#L68-L115)                   |
| Exec background | 15 秒后自动转后台，可 list、output、kill                                                | PARTIAL | [exec.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/exec.rs#L118-L176)                  |
| Task            | start、list、output、kill、restart                                                      | PARTIAL | [execute.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/agent_loop/execute.rs#L304-L345) |
| Dev server      | pattern 或 port readiness，加 periodic health                                           | PARTIAL | [dev_server.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/dev_server.rs#L1-L147)        |
| Webfetch        | http 或 https，20 秒 timeout，50K 字符输出                                              | PARTIAL | [webfetch.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/webfetch.rs#L1-L47)             |
| Websearch       | DuckDuckGo HTML 单源                                                                    | PARTIAL | [websearch.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/websearch.rs#L1-L60)           |
| Context         | file、directory、web、note 和 image URL                                                 | PARTIAL | [context.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/context.rs#L1-L145)              |
| Hooks           | pre tool、post tool、session start、stop、notification、teammate idle 和 task completed | PARTIAL | [hooks.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/hooks.rs#L1-L105)                  |
| Approval broker | oneshot 等用户回复                                                                      | PARTIAL | [approval.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/approval.rs#L1-L50)             |

已确认细节错误：

- `exec` 用 agent workdir 评估相对路径安全性，却按 `params.path` 执行。攻击者可以让同一相对命令在另一个目录解析为不同目标。[exec.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/exec.rs#L68-L111)
- `timeout_ms` 大于 15 秒时，15 秒后只返回 background task，不再执行 hard timeout。默认 120 秒也不会自动杀死后台进程。[exec.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/exec.rs#L118-L145)
- `task start` 直接进入 dev server，没有调用 shell safety 和 approval。
- Hook runner 只阻断 Verdict Deny。Verdict Ask 被当作可执行，跳过用户审批。[hooks.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/hooks.rs#L72-L99)
- Context 的 absolute file path 直接读取，不调用 filesystem guard，可把凭证文件内容注入 prompt 并持久化到 session context。
- Webfetch 和 image URL 只校验 scheme，不阻断 loopback、RFC1918、link-local、Unix socket gateway 或 cloud metadata endpoint。
- Webfetch 先把完整 body 读入内存，再做文本 trim。
- file freshness 使用秒级时间加 size，同一秒内同大小修改可以漏检。
- task output 只有 64KB 合并 buffer，stdout 和 stderr 顺序不稳定。
- dev server readiness timeout 后没有终止已启动进程。未显式 port 时，解析出的 port 没有写回 task health state。

## 3.6 Checkpoint、rewind、worktree 和 diff

| 功能            | 当前细节                                                         |    状态 | 证据                                                                                                 |
| --------------- | ---------------------------------------------------------------- | ------: | ---------------------------------------------------------------------------------------------------- |
| Shadow Git      | 每个 workspace 对应私有 bare repo，不污染项目 Git metadata       |    PASS | [checkpoint.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/checkpoint.rs#L1-L80)   |
| Turn checkpoint | 用户消息落盘后异步创建 checkpoint                                | PARTIAL | [llm_task.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/ws/llm_task.rs#L118-L143)       |
| Rewind          | 找 message id 对应 commit，hard reset 后截断 JSONL               | PARTIAL | [session_ops.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/ws/session_ops.rs#L18-L30)   |
| Snapshot scope  | 排除 ignored files、node modules、target 和 kxen worktrees       | PARTIAL | [checkpoint.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/checkpoint.rs#L1-L15)   |
| Worktree create | 校验 ASCII name，创建 branch 和 worktree                         |    PASS | [worktree.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/worktree.rs#L18-L35)      |
| Worktree reuse  | path 已存在时直接返回预期 branch 信息                            | PARTIAL | [worktree.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/worktree.rs#L18-L35)      |
| Worktree remove | `--force` 删除 worktree，可选 `-D` 删除 branch                   |    FAIL | [worktree.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/worktree.rs#L37-L47)      |
| Gitignore       | 创建 worktree 时自动写入忽略项                                   | PARTIAL | [worktree.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/worktree.rs#L131-L145)    |
| Diff panel      | session snapshot、repo diff、file diff 和 worktree diff stat     | PARTIAL | [Dock.tsx](file:///Users/xiaobai/Code/SelfCode/kxen/src/components/Dock.tsx#L220-L297)               |
| Inline review   | 无 diff comment、accept/reject hunk、PR review 和 merge workflow |    FAIL | [DockWorktree.tsx](file:///Users/xiaobai/Code/SelfCode/kxen/src/components/DockWorktree.tsx#L1-L135) |

已确认细节错误：

- checkpoint 是 fire-and-forget。agent 可在 commit 完成前修改文件，checkpoint 不再严格表示 turn 前状态。
- rewind 对整个工作目录执行 `git reset --hard`，没有确认当前 dirty tree、其他 session、外部编辑器和 active agents。
- ignored file 不在 checkpoint 中，rewind 后 workspace 不是完整快照。
- worktree 已存在时不验证实际 registered branch、HEAD 和 ownership。
- worktree remove 强制删除，没有 dirty guard 和 approval。
- `diff_stat` 比较 branch，而不是完整 staged、unstaged 和 untracked 状态。
- session snapshot 只跟踪主 context 中的 filesystem tools，exec、MCP、team、subagent 和外部进程修改不会完整进入面板。

相对竞品：

- Gemini CLI 和 Cline 的 checkpoint 明确与 tool call 或对话状态绑定。
- Roo Code 支持 checkpoint rewind 后恢复完整 history。
- Cursor、Windsurf Arena、Conductor 和 Vibe Kanban 把 worktree 与 agent task 绑定，kxen 只是一个可调用工具和 Dock 子面板。

## 3.7 Knowledge、rules、skills、commands 和 memory

| 功能                   | 当前细节                                                |    状态 | 证据                                                                                                      |
| ---------------------- | ------------------------------------------------------- | ------: | --------------------------------------------------------------------------------------------------------- |
| OKF kinds              | rule、reference、skill、command、note、memory、history  |    PASS | [knowledge/mod.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/knowledge/mod.rs#L1-L90)        |
| Scope                  | project 和 personal 双 scope                            |    PASS | [store.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/knowledge/store.rs#L1-L110)             |
| Rule compatibility     | 读取 AGENTS、CLAUDE、GEMINI 和 Cursor root rules        |    PASS | [scan.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/knowledge/scan.rs#L1-L35)                |
| Nearby rules           | 按涉及文件注入就近 directory rules                      |    PASS | [render.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/knowledge/render.rs#L120-L160)         |
| Dynamic globs          | 根据 involved files 激活匹配内容                        |    PASS | [render.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/knowledge/render.rs#L45-L95)           |
| Injection tiers        | full、index、untrusted downgrade 和 unmatched downgrade |    PASS | [render.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/knowledge/render.rs#L45-L80)           |
| Skills                 | 支持 needs、arguments 和递归 cap                        |    PASS | [skills.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/skills.rs#L1-L90)                |
| Commands               | slash command 展开                                      |    PASS | [commands.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/commands.rs#L1-L80)            |
| Knowledge tool         | add、list、remove                                       |    PASS | [execute.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/agent_loop/execute.rs#L80-L110) |
| Memory retrieval       | token overlap score 加日期 fallback                     | PARTIAL | [render.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/knowledge/render.rs#L1-L45)            |
| Session delete distill | 删除前把 transcript 蒸馏为 notes                        | PARTIAL | [distill.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/knowledge/distill.rs#L1-L100)         |
| Periodic consolidation | App 存活时每 30 分钟扫描一次                            | PARTIAL | [consolidate.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/knowledge/consolidate.rs#L1-L70)  |
| Trust                  | project config 和部分 project knowledge 有 path trust   | PARTIAL | [trust.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/core/trust.rs#L1-L100)                  |

已确认细节错误：

- trust 主要保护 render、project hooks 和 MCP config。skill needs、command、custom role 和部分 knowledge operation 不共享同一执行信任模型。
- trust 记录基于 path string，没有 canonical path、content hash、revoke 和 per-capability scope。
- project 和 personal 使用相同 kind、slug 时 project first wins，personal 内容会被隐藏。
- 没有 frontmatter 的 plain Markdown 在 enable 操作中不能可靠写回状态。
- skill move 只移动主文件，关联 resources 不随主文件移动。
- slugify 只保留 ASCII。纯中文标题统一退化为 `note` 并覆盖。
- description 直接进入 frontmatter，没有 escaping。
- memory retrieval 没有 embedding、semantic index、conflict resolution 和 source quality。
- consolidation 即使 distill 失败也可以推进 watermark，失败 session 不会自动重试。
- distill 在 session 删除时使用 active workspace，而不是 session metadata 中的 directory。
- notes 写入会直接修改项目工作树，没有单独 review 或 approval surface。

相对竞品：

- Claude Code 的规则、skills、hooks、plugins 和 subagents 形成统一扩展层。
- Windsurf 同时提供 rules、automatic memories、workflows 和 skills。
- Codex 以 project instructions、skills 和后台 memories 组合。
- kxen 的七类 OKF 和四级注入是结构化优势，但 trust 和持久化正确性没有覆盖全部消费路径。

## 3.8 MCP 和 LSP

| 功能              | 当前细节                                                          |    状态 | 证据                                                                                                       |
| ----------------- | ----------------------------------------------------------------- | ------: | ---------------------------------------------------------------------------------------------------------- |
| MCP config        | user 和 trusted project 两层配置                                  |    PASS | [config.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/mcp/config.rs#L1-L80)                   |
| MCP transport     | stdio JSON-RPC line transport                                     |    PASS | [transport.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/mcp/transport.rs#L1-L78)             |
| MCP handshake     | initialize、initialized、tools list                               |    PASS | [client.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/mcp/client.rs#L1-L93)                   |
| MCP tools         | 每轮把 server tools 加入 model tool defs                          |    PASS | [run.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/agent_loop/run.rs#L63-L77)           |
| MCP call          | `mcp__server__tool` 前缀分发                                      |    PASS | [execute.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/agent_loop/execute.rs#L288-L302) |
| MCP lifecycle     | status、restart 和 startup connect                                | PARTIAL | [mcp/mod.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/mcp/mod.rs#L1-L105)                    |
| MCP remote        | HTTP、SSE、OAuth、resources、prompts、roots 和 sampling           |    FAIL | [config.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/mcp/config.rs#L40-L75)                  |
| LSP process       | workspace 级 lazy rust-analyzer                                   |    PASS | [lsp/mod.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/lsp/mod.rs#L1-L128)                    |
| LSP capability    | didOpen、didChange 和 diagnostics                                 | PARTIAL | [process.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/lsp/process.rs#L1-L100)                |
| Code intelligence | 无 hover、definition、references、rename、symbols 和 code actions |    FAIL | [lsp](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/lsp)                                          |

已确认细节错误：

- MCP tools 在 resident tool permission 过滤之后追加。Readonly subagent 和 plan-only teammate 可以看到所有 MCP tools，server write tool 不受角色 profile 约束。
- MCP tool 没有 per-tool allow、ask、deny、annotation 和 approval。
- MCP 只在 App 启动时读取一次。workspace switch 不 reload project MCP。
- project stdio command 没有明确 workspace cwd，stderr 被丢弃。
- MCP manager call 失败后不会可靠把 client 标为 down，也没有自动 reconnect。
- server 和 tool name 通过双下划线编码，原始 name 含同样分隔符时解析有歧义。
- MCP result 没有输出 cap，可直接撑大 context。
- LSP 只处理 Rust，kxen 自己的 TypeScript 和 TSX 无覆盖。
- file URI 没有标准 percent encoding。
- diagnostics store 没有 document version。已有 entry 可返回 stale result。
- `try_lock` 失败时 change notification 可以静默丢失。
- workspace switch 替换 AppState LSP，但已有 run、team 和 captured dependencies 仍持有旧 Arc。

相对竞品：

- Gemini CLI、Cursor、Windsurf、Cline 和 OpenCode 支持 remote MCP 形态。
- Claude Code MCP 还与 hooks、plugins 和 tool discovery 组合。
- OpenCode 的 LSP 覆盖多语言并用于 diagnostics。
- kxen 的 MCP 和 LSP 已经从 absent 进入可调用状态，但仅达到最小闭环。

## 3.9 Voice、notifications、settings 和 diagnostics

| 功能                | 当前细节                                                                  |    状态 | 证据                                                                                                             |
| ------------------- | ------------------------------------------------------------------------- | ------: | ---------------------------------------------------------------------------------------------------------------- |
| Apple speech        | SFSpeechRecognizer 流式 partial 和 final                                  |    PASS | [objc.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/voice/objc.rs#L47-L173)                         |
| Provider speech     | 录音后上传 OpenAI 或 xAI compatible transcription                         |    PASS | [provider.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/voice/provider.rs#L1-L180)                  |
| Voice fallback      | engine 加 fallback chain                                                  | PARTIAL | [voice/mod.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/voice/mod.rs#L41-L125)                     |
| PTT                 | UI press and hold 加空格快捷键                                            |    PASS | [voice-ptt.ts](file:///Users/xiaobai/Code/SelfCode/kxen/src/components/composer/voice-ptt.ts#L1-L127)            |
| Voice session scope | active recorder 为进程级 static，event 无 session id                      |    FAIL | [voice/mod.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/voice/mod.rs#L41-L100)                     |
| Notifications       | 顶栏中心加 OS notification，内存 cap 50                                   |    PASS | [NotificationCenter.tsx](file:///Users/xiaobai/Code/SelfCode/kxen/src/components/NotificationCenter.tsx#L1-L101) |
| Settings            | theme、provider、routing、usage、knowledge、voice、MCP 和 diagnostics     |    PASS | [Settings.tsx](file:///Users/xiaobai/Code/SelfCode/kxen/src/pages/Settings.tsx#L1-L180)                          |
| Send policy setting | UI 可以写 queue 或 interrupt                                              |    FAIL | [ops.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/ws/ops.rs#L219-L250)                             |
| Voice setting       | UI 可选 engine 和设置部分 provider key                                    | PARTIAL | [VoiceSection.tsx](file:///Users/xiaobai/Code/SelfCode/kxen/src/components/settings/VoiceSection.tsx#L1-L106)    |
| Diagnostics export  | 提供配置和状态导出 RPC                                                    |    PASS | [ops.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/ws/ops.rs#L1-L220)                               |
| Doctor              | 列出 runtime、data directory、config directory 和各账号 credential status | PARTIAL | [doctor.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/doctor.rs#L1-L67)                             |
| Schedule UI         | 没有 schedule list、create、pause、history 和 failure UI                  |    FAIL | [ops.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/ws/ops.rs#L1-L35)                                |

已确认细节错误：

- Apple request 没有设置 `requiresOnDeviceRecognition`。UI 的 offline 或 zero cost 声明不能从当前代码成立。
- `voice.set_engine` 重建整个 voice table，保存 engine 和 fallback 时会删除现有 locale 和 transcribe model。
- Voice fallback chain 有 backend config，但 UI 没有完整编辑面。
- 任一连接都能 start 或 stop 进程级 recording。
- notification collector 遇到 broadcast lag error 后会退出，不再收集后续通知。
- AppState activity、notifications、usage、draft 和 transcript 都是内存状态。

## 3.10 配置、事件流、性能和发布

| 功能            | 当前细节                                     |    状态 | 证据                                                                                          |
| --------------- | -------------------------------------------- | ------: | --------------------------------------------------------------------------------------------- |
| Config layering | default、user、trusted project merge         | PARTIAL | [config.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/core/config.rs#L118-L148)  |
| Statusline      | git、model、goal、tokens、context 和 tasks   | PARTIAL | [settings.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/ws/settings.rs#L1-L80)   |
| EventBus        | Tokio broadcast，capacity 256                | PARTIAL | [event.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/core/event.rs#L1-L39)       |
| Stream sequence | per stream monotonic seq                     | PARTIAL | [ws/mod.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/ws/mod.rs#L1-L45)          |
| Reconnect       | 断线 1 秒后重连并恢复 subscriptions          |    FAIL | [client.ts](file:///Users/xiaobai/Code/SelfCode/kxen/src/lib/client.ts#L130-L149)             |
| Frontend build  | 4565 modules，production bundle 可生成       |    PASS | 本轮 TESTED                                                                                   |
| Code splitting  | Shiki language 和 Mermaid 形成大量 chunks    | PARTIAL | 本轮 TESTED                                                                                   |
| Rust compile    | all targets 可通过                           |    PASS | 本轮 TESTED                                                                                   |
| Rust tests      | 150 passed                                   |    PASS | 本轮 TESTED                                                                                   |
| Rust format     | 当前源码和 examples 存在大量 rustfmt diff    |    FAIL | 本轮 TESTED                                                                                   |
| Frontend tests  | 23 tests 执行通过，但命令因缺少 jsdom 退出 1 |    FAIL | 本轮 TESTED                                                                                   |
| Frontend check  | 122 个文件格式检查和 78 个文件 lint 检查通过 |    PASS | 本轮 TESTED                                                                                   |
| Auto updater    | 产品文档声明 updater，依赖和初始化不存在     |    FAIL | [Cargo.toml](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/Cargo.toml)                   |
| Signing         | dmg target 存在，signing identity 为 null    | PARTIAL | [tauri.conf.json](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/tauri.conf.json#L31-L39) |

已确认细节错误：

- Config merge 没有复制 `send_when_running`，所以 UI 写入后 load 仍回到默认 queue。
- 反序列化缺省 `limits` 会产生默认 global value，project config 即使没写 limits 也会覆盖 user value。
- 缺省 statusline 不是空值，project config 没写 statusline 时也会覆盖 user setting。
- WebSocket client 在遍历 subscriptions Map 时删除旧 key，再由 `openSubscription` 插入新 key。JavaScript Map 迭代会访问新插入 entry，可形成持续 reopen。
- EventBus lag 直接断开当前 WebSocket loop，没有 gap response、resume cursor 和 replay。
- topic dispatch 用 `find`，同一连接内同 topic 只有第一个 subscription 收到 event。
- stream sequence map 没有完成后清理。
- approval id、session id、message id 和 cron id 使用毫秒加 process id，同毫秒创建会碰撞。
- approval 没有 timeout。cancel 后 pending sender 仍可保留。
- 当前 frontend build 报告多个大于 500KB 的 chunks，最大输出 chunk 约 779.87KB。

## 4. P0 差距清单

| ID    | 差距                                                                                                       | 影响                                                                   | 静态证据                                                                                                                                                                                                                                                                                                                                                                                                       | 对标                                                              |
| ----- | ---------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| P0-01 | 本地 WebSocket 无 token、Origin 和 client identity                                                         | 本机任意网页或进程可调用 mutation RPC                                  | [ws/mod.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/ws/mod.rs#L48-L88)                                                                                                                                                                                                                                                                                                                          | Codex 使用 OS sandbox 和 approval boundary                        |
| P0-02 | 每个 WebSocket 连接接收所有直接 LLM stream                                                                 | 跨 session prompt、tool 和 output 泄露                                 | [stream.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/ws/stream.rs#L11-L45)                                                                                                                                                                                                                                                                                                                       | 成熟产品按 task 或 session 隔离流                                 |
| P0-03 | session id、team session id、member name、workflow run id 和 worktree remove name 缺少统一 path validation | 路径穿越、覆盖或读取 data directory 外文件                             | [session.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/core/session.rs#L60-L69) [manager.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/team/manager.rs#L70-L95) [workflow_journal.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/workflow_journal.rs#L7-L28) [worktree.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/worktree.rs#L37-L47) | 成熟 runtime 使用 opaque id 和 canonical root guard               |
| P0-04 | `exec` safety cwd 和真实 execution cwd 不一致                                                              | 同一命令可以通过检查后在另一个目录作用于危险目标                       | [exec.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/exec.rs#L68-L111)                                                                                                                                                                                                                                                                                                                       | Claude Code 和 OpenCode 权限规则绑定 tool input                   |
| P0-05 | `task start` 不经过 shell safety 和 approval                                                               | 任意命令可从 dev server 路径直接启动                                   | [execute.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/agent_loop/execute.rs#L304-L345)                                                                                                                                                                                                                                                                                                     | Cline 和 Goose 采用 per-tool permission                           |
| P0-06 | Hook 的 Ask verdict 直接执行                                                                               | 需要用户确认的 command 在 hook 路径静默运行                            | [hooks.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/hooks.rs#L72-L99)                                                                                                                                                                                                                                                                                                                      | Claude Code hooks 支持结构化 decision                             |
| P0-07 | Context file 绕过 path guard，Web 和 image context 无 SSRF guard                                           | 可读敏感文件并访问私网和 metadata service                              | [context.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/context.rs#L47-L145)                                                                                                                                                                                                                                                                                                                 | Codex workspace write 默认关闭网络                                |
| P0-08 | MCP tools 在 permission filter 后追加                                                                      | readonly 或 plan-only agent 可调用 MCP 写工具                          | [run.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/agent_loop/run.rs#L63-L77)                                                                                                                                                                                                                                                                                                               | Claude Code subagent 有独立 tools 和 permissions                  |
| P0-09 | cron 绕过 active run guard，为同一 session 并发 run                                                        | JSONL 交叉写、active token 覆盖、错误 cancel 和 history corruption     | [main.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/main.rs#L214-L241)                                                                                                                                                                                                                                                                                                                            | 调度产品为每个 task 建独立 workspace 或 execution                 |
| P0-10 | rewind hard reset 整个 workspace 且无状态保护                                                              | 丢失用户编辑、其他 session 和 agent 结果                               | [checkpoint.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/checkpoint.rs#L81-L88)                                                                                                                                                                                                                                                                                                            | Gemini 和 Cline checkpoint 与会话动作绑定                         |
| P0-11 | worktree force remove 和 branch delete 无 approval 或 dirty guard                                          | 未提交工作和 branch 可直接丢失                                         | [worktree.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/tools/worktree.rs#L37-L47)                                                                                                                                                                                                                                                                                                                | Conductor 把 review、merge、archive 作为 workspace lifecycle      |
| P0-12 | 无 OS sandbox 和 network confinement                                                                       | shell、MCP 和 hook 获得 App 用户完整权限                               | [Cargo.toml](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/Cargo.toml)                                                                                                                                                                                                                                                                                                                                    | Codex 和 Gemini 提供 OS 或 container sandbox                      |
| P0-13 | 主 session 和 team LLM 请求不进入 MRM acquire                                                              | global concurrency 和 provider RPM 对主产品路径不生效                  | [run.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/agent_loop/run.rs#L99-L169)                                                                                                                                                                                                                                                                                                              | 多 agent 产品显式管理 concurrent task                             |
| P0-14 | TeamManager 和 SessionExtras 跨 workspace 与 session 共享 stale state                                      | agent 可在错误 workspace 工作，deferred tools 和 todos 跨 session 泄露 | [main.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/main.rs#L72-L99)                                                                                                                                                                                                                                                                                                                              | Cursor、Windsurf、Conductor 和 Vibe 使用 task workspace isolation |

## 5. P1 和 P2 差距清单

### 5.1 P1

| ID    | 差距                                                          | 结果                                                |
| ----- | ------------------------------------------------------------- | --------------------------------------------------- |
| P1-01 | Config merge 丢失 send policy 并意外覆盖 limits 和 statusline | 设置页保存结果与下次 load 不一致                    |
| P1-02 | model 和 account 是全局状态                                   | 切 session 或恢复 session 时 model 漂移             |
| P1-03 | image 不持久化                                                | reload、fork、export 和后续历史失真                 |
| P1-04 | streamed reasoning 不持久化                                   | reload 后时间线和审计信息丢失                       |
| P1-05 | tool transcript 只保存 summary                                | 无法重放精确 arguments 和完整 result                |
| P1-06 | manual compaction 把非 user role 统一为 assistant             | tool、system、image 和 reasoning 结构丢失           |
| P1-07 | auto compaction 不写回 session                                | 每次重新打开都可重复花费 compaction                 |
| P1-08 | checkpoint 异步 race                                          | checkpoint 不保证为 tool execution 前状态           |
| P1-09 | session delete 不终止相关 runtime                             | 已删除 session 仍可被 run 或 cron 写回              |
| P1-10 | fork 保留原 message id                                        | checkpoint label 和 UI identity 跨 session 重复     |
| P1-11 | id 使用毫秒加 PID                                             | 高频 create、message、approval 和 schedule 存在碰撞 |
| P1-12 | usage 只取最后 request                                        | cost、tokens per second 和 goal budget 低估         |
| P1-13 | pending queue 仅内存                                          | App crash 后排队消息丢失                            |
| P1-14 | WebSocket lag 无 replay                                       | 慢客户端丢流并断线                                  |
| P1-15 | reconnect 修改正在遍历的 Map                                  | subscription 恢复可持续循环                         |
| P1-16 | raw Markdown HTML 无 sanitizer                                | model output 可注入 UI HTML 和 style                |
| P1-17 | approval 无 timeout 和 cancel cleanup                         | pending approval 泄漏或被碰撞 id 覆盖               |
| P1-18 | Team inbox read/truncate race                                 | 并发消息丢失                                        |
| P1-19 | Team restore 不恢复 loop                                      | 持久化只恢复数据，不恢复工作                        |
| P1-20 | Team task 缺少 failed、canceled 和 reassign                   | 失败 dependency 可永久阻塞                          |
| P1-21 | teammate idle hook 拒绝不 notify                              | agent 可永久 idle                                   |
| P1-22 | Workflow journal 无脚本和输入版本                             | 相同 run id 可复用过期或错误结果                    |
| P1-23 | MCP 仅 stdio，且不随 workspace reload                         | project tool 集与当前 workspace 不一致              |
| P1-24 | MCP 无 output cap 和 per-tool approval                        | context 膨胀并扩大写权限                            |
| P1-25 | LSP 仅 Rust diagnostics                                       | TypeScript、TSX 和其他语言无 code intelligence      |
| P1-26 | Trust 不覆盖 custom roles、skill needs 和 commands            | 未信任项目仍可影响 agent 行为                       |
| P1-27 | 中文知识 slug 退化                                            | 不同内容可覆盖同一文件                              |
| P1-28 | consolidation failure 仍推进 watermark                        | 记忆永久漏蒸馏                                      |
| P1-29 | voice active 为进程全局且 event 无 session id                 | 多 session PTT 互相停止或串流                       |
| P1-30 | Apple speech 未强制 on-device                                 | offline 和 zero cost 产品描述不成立                 |
| P1-31 | 普通附件只保存 basename                                       | 引用错误文件或读取失败                              |
| P1-32 | 新 session draft 的空 id 残留                                 | 已发送文本出现在下一次新 session                    |
| P1-33 | dev server timeout 不 kill                                    | 失败启动仍留后台进程                                |
| P1-34 | exec hard timeout 在 auto background 后失效                   | 长任务可无限运行                                    |
| P1-35 | updater 和 release signing 未闭合                             | 无可靠升级和可信分发链                              |

### 5.2 P2

| ID    | 差距                                        | 结果                                               |
| ----- | ------------------------------------------- | -------------------------------------------------- |
| P2-01 | Read 无 offset 和 paging                    | 大文件只能读取前段                                 |
| P2-02 | Search cap 不返回完整 total 和 continuation | agent 不知道是否漏结果                             |
| P2-03 | File freshness 为秒级加 size                | 同秒同大小修改漏检                                 |
| P2-04 | Tool calls 固定顺序执行                     | 独立 reads 和 searches 无并行收益                  |
| P2-05 | Goal complete 只要求非空 evidence           | 不能证明验收完成                                   |
| P2-06 | Goal paused 时间仍进入 wall budget          | pause 语义不准确                                   |
| P2-07 | Goal wall budget 仅在 tool round 后检查     | 长 LLM stream 无轮内终止                           |
| P2-08 | Goal statusline 使用 global focus           | UI 可显示其他 session goal                         |
| P2-09 | Memory retrieval 仅 token overlap           | 语义召回和冲突处理不足                             |
| P2-10 | Notifications 和 usage 不持久化             | 重启后运营和诊断历史消失                           |
| P2-11 | Schedule 无 UI 和 execution history         | 用户无法审计和暂停后台任务                         |
| P2-12 | Build chunks 体积大                         | 首次载入和内存占用高于必要值                       |
| P2-13 | Doctor 主要聚焦 credential                  | 缺少 MCP、LSP、checkpoint、MRM 和 event bus health |
| P2-14 | 文档与实现漂移                              | 用户无法从 README 和 PRD 得到当前事实              |

## 6. 竞品能力总表

状态说明：

- PASS：当前官方资料确认该产品提供此能力。
- PARTIAL：能力存在，但形态不同或只覆盖子集。
- INHERITED：编排产品依赖底层 coding agent 提供。
- UNKNOWN：本轮资料不足。

| 产品         | 产品形态                     | 并行和隔离                              | Checkpoint 和恢复                   | 权限和 sandbox                    | MCP 和扩展                                              | Code intelligence       | 相对 kxen 的主要差异                                       |
| ------------ | ---------------------------- | --------------------------------------- | ----------------------------------- | --------------------------------- | ------------------------------------------------------- | ----------------------- | ---------------------------------------------------------- |
| Claude Code  | CLI 加桌面和云任务           | PASS，subagent 和 agent teams           | PASS，checkpoint 和 session control | PASS，细粒度 permissions 和 hooks | PASS，MCP、skills、plugins                              | PASS，code intelligence | 运行边界和扩展成熟度领先，kxen 多 provider 和 voice 更集中 |
| OpenAI Codex | CLI、App 和 cloud            | PASS，subagents、worktrees、cloud tasks | UNKNOWN                             | PASS，OS sandbox 和 approval      | PARTIAL，skills 和 schedules 已确认，MCP 本轮未单独量化 | PARTIAL                 | sandbox、cloud isolation 和 parallel task 领先             |
| Gemini CLI   | CLI                          | PARTIAL，agent extension 生态           | PASS，shadow Git checkpoint         | PASS，Seatbelt 或 container       | PASS，stdio、SSE、HTTP MCP 和 extensions                | PARTIAL                 | checkpoint 和 sandbox 语义领先                             |
| OpenCode     | CLI、TUI、server             | PASS，primary agent 和 subagent         | UNKNOWN                             | PASS，allow、ask、deny rules      | PASS，plugins 和 local/remote MCP                       | PASS，多语言 LSP        | provider、LSP、permission 和 plugin 领先                   |
| Goose        | Desktop、CLI、API            | PASS，subagents                         | PARTIAL，session 可恢复             | PASS，Always、Ask、Never          | PASS，MCP extensions 和 recipes                         | PARTIAL                 | provider、extensions 和 recipes 领先                       |
| Cursor       | IDE 和 cloud agents          | PASS，cloud VM 和 worktrees             | UNKNOWN                             | PASS，isolated cloud environment  | PASS，MCP                                               | PASS，IDE native        | IDE intelligence、browser、cloud agent 和 worktree 领先    |
| Windsurf     | IDE agent                    | PASS，Arena 独立 sessions 和 worktrees  | UNKNOWN                             | PARTIAL，mode 和 policy           | PASS，MCP、skills、workflows                            | PASS，IDE native        | Arena、memory、workflows 和 IDE integration 领先           |
| Cline        | VS Code、CLI、SDK            | PARTIAL                                 | PASS，persistent shadow Git         | PASS，per-tool auto approve       | PASS，MCP 和 skills                                     | PASS，IDE 加 browser    | checkpoint、browser 和 granular approvals 领先             |
| Roo Code     | VS Code agent                | PASS，orchestrator mode                 | PASS，checkpoint rewind             | PARTIAL                           | PASS，MCP 和 skills                                     | PASS，IDE 加 browser    | orchestrator、browser 和 context recovery 领先             |
| Conductor    | macOS multi-agent workspace  | PASS，workspace 和 worktree             | PARTIAL，workspace lifecycle        | INHERITED                         | INHERITED，多 harness                                   | INHERITED               | workspace、review、diff 和 merge 是第一等能力              |
| Vibe Kanban  | Web 或 desktop orchestration | PASS，issue 和 workspace                | PARTIAL，workspace session          | INHERITED                         | INHERITED，多 executor                                  | INHERITED               | Kanban、attempt、diff review 和 executor adapters 领先     |

## 7. 逐产品详细对比

### 7.1 Claude Code

官方资料确认：

- 规则、skills、code intelligence、MCP、isolated subagents、agent teams、hooks、plugins 组成统一扩展层。[Exa: Claude Code features](https://code.claude.com/docs/en/features-overview)
- subagent 有独立 context、system prompt、tools 和 permissions。[Exa: Claude Code subagents](https://code.claude.com/docs/en/subagents)
- agent teams 使用共享 tasks 和 peer messaging，并明确列出 resume、coordination 和 shutdown 限制。[Exa: Claude Code agent teams](https://code.claude.com/docs/en/agent-teams)
- hooks 支持 command、HTTP、prompt 和 subagent handler，并返回结构化 decision。[Exa: Claude Code hooks](https://code.claude.com/docs/en/hooks)
- checkpoint 在每个 user prompt 前保存 code 状态，并可分别恢复 code、conversation 或两者。[Exa: Claude Code checkpointing](https://code.claude.com/docs/en/checkpointing)

差异：

- Claude Code 领先：permission model、hook event 和 handler、extension packaging、code intelligence、agent team isolation。
- kxen 领先或差异化：跨多家 CLI 的 credential reuse、显式三维 goal budget、Apple speech 加 provider fallback、单一桌面 Dock。
- kxen 必须补齐：MCP 权限继承、team session isolation、hook Ask approval、session-scoped runtime。

### 7.2 OpenAI Codex

官方资料确认：

- local execution 使用 OS enforced sandbox，workspace write 默认关闭网络，cloud task 使用隔离 container。[Exa: Codex security](https://developers.openai.com/codex/agent-approvals-security)
- local environments 支持 worktree setup 和 project actions。[Exa: Codex local environments](https://developers.openai.com/codex/app/local-environments)
- subagents 可并行工作并在主 thread 中暴露结果。[Exa: Codex subagents](https://developers.openai.com/codex/concepts/subagents)
- Codex App 是并行 agent 和长任务 command center。[Exa: Codex App](https://openai.com/index/introducing-the-codex-app/)
- 开源协议明确 WorkspaceWrite 的 network access 默认 false。[gh_grep: Codex sandbox policy](https://github.com/openai/codex/blob/main/codex-rs/protocol/src/protocol.rs)

差异：

- Codex 领先：OS sandbox、network deny by default、cloud isolation、worktree task 和 parallel task UX。
- kxen 领先或差异化：本地多 provider 和多账号池、语音、显式 OKF 和 in-process QuickJS workflow。
- kxen 必须补齐：sandbox profile、network policy、task workspace ownership、local RPC authentication。

### 7.3 Gemini CLI

官方资料确认：

- AI 文件修改前创建 shadow Git checkpoint，快照包括 project、conversation 和 tool call 状态。[Exa: Gemini checkpointing](https://google-gemini.github.io/gemini-cli/docs/cli/checkpointing.html)
- 支持 macOS Seatbelt 和 Docker 或 Podman sandbox。[Exa: Gemini sandbox](https://google-gemini.github.io/gemini-cli/docs/cli/sandbox.html)
- MCP 支持 stdio、SSE 和 streamable HTTP，并覆盖 tools 和 resources。[Exa: Gemini MCP](https://google-gemini.github.io/gemini-cli/docs/tools/mcp-server.html)
- extensions 可封装 prompts、MCP 和 commands。[Exa: Gemini extensions](https://google-gemini.github.io/gemini-cli/docs/extensions/)
- checkpoint 实现使用 shadow Git service。[gh_grep: Gemini Git service](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/services/gitService.ts)

差异：

- Gemini 领先：checkpoint 原子语义、sandbox、remote MCP、extension packaging。
- kxen 领先或差异化：桌面多面板、team、goal 和多 provider credentials。
- kxen 必须补齐：checkpoint 和 conversation 原子关联、rewind preview、sandbox 和 remote MCP。

### 7.4 OpenCode

官方资料确认：

- primary agents 和 subagents 有 Build、Plan 等模式。[Exa: OpenCode agents](https://opencode.ai/docs/agents/)
- permission 支持 granular allow、ask 和 deny rules。[Exa: OpenCode permissions](https://opencode.ai/docs/permissions/)
- 内置 LSP servers 覆盖多个语言。[Exa: OpenCode LSP](https://opencode.ai/docs/lsp/)
- plugins 可消费 events，来源包括 local 和 npm。[Exa: OpenCode plugins](https://opencode.ai/docs/plugins/)
- provider catalog 通过 [https://models.dev](https://models.dev) 提供 75+ providers。[Exa: OpenCode providers](https://opencode.ai/docs/providers/)
- permission 和 LSP 实现可从源码确认。[gh_grep: OpenCode permission](https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/permission/index.ts) [gh_grep: OpenCode LSP](https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/lsp/lsp.ts)

差异：

- OpenCode 领先：provider breadth、multi-language LSP、plugin system、permission rules、remote MCP。
- kxen 领先或差异化：native macOS app、goal、team、workflow journal、voice 和 knowledge UI。
- kxen 必须补齐：provider adapter maturity、LSP surface、plugin or SDK、permission unification。

### 7.5 Goose

官方资料确认：

- 提供 native desktop、CLI、API、MCP extensions、recipes、sessions 和 provider abstraction。[Exa: Goose docs](https://goose-docs.ai/)
- subagents 可处理独立任务。[Exa: Goose subagents](https://goose-docs.ai/docs/tutorials/subagents/)
- GitHub 项目说明支持多 provider 和大量 extensions。[Exa: Goose GitHub](https://github.com/aaif-goose/goose)
- tool permission 支持 Always Allow、Ask Before 和 Never Allow。[gh_grep: Goose permission](https://github.com/aaif-goose/goose/blob/main/crates/goose/src/config/permission.rs)
- recipes 用 YAML 表达 instructions、extensions、parameters、settings 和 sub-recipes。[Exa: Goose recipes](https://block-goose.mintlify.app/concepts/recipes)

差异：

- Goose 领先：MCP extension ecosystem、provider breadth、recipe distribution、permission setting。
- kxen 领先或差异化：三维 goal budget、shadow checkpoint、team task board、OKF injection tiers 和 Apple speech。
- kxen 必须补齐：extension lifecycle、permission enforcement 和 recipe or workflow portability。

### 7.6 Cursor

官方资料确认：

- background agents 运行在隔离 cloud VM，可生成 PR。[Exa: Cursor background agents](https://cursor.com/help/ai-features/background-agents)
- worktrees 用于 parallel agents 和 environment setup。[Exa: Cursor worktrees](https://cursor.com/docs/configuration/worktrees.md)
- rules 提供 project、global 和 agent instruction 持久化。[Exa: Cursor rules](https://cursor.com/docs/rules)
- cloud agents 支持 many parallel agents、MCP、multi-repo、browser 和 computer use。[Exa: Cursor cloud agents](https://cursor.com/docs/cloud-agent)
- MCP 支持 tools、prompts、resources、roots、elicitation 和多 transport。[Exa: Cursor MCP](https://cursor.com/docs/mcp)

差异：

- Cursor 领先：IDE semantic context、cloud isolation、browser、multi-repo 和 task worktree。
- kxen 领先或差异化：本地自建 runtime、多 provider account pool、goal contract、voice。
- kxen 必须补齐：worktree ownership、browser tool、multi-language intelligence、task level model and permission persistence。

### 7.7 Windsurf

官方资料确认：

- memories、rules、instructions 和 workflows 形成持久上下文层。[Exa: Windsurf memories](https://docs.windsurf.com/windsurf/cascade/memories)
- MCP 支持 stdio、HTTP 和 SSE。[Exa: Windsurf MCP](https://docs.windsurf.com/plugins/cascade/mcp)
- Arena 同时运行多个独立 Cascade session，每个 session 使用自己的 worktree，并让用户选择结果。[Exa: Windsurf Arena](https://docs.windsurf.com/windsurf/cascade/arena)
- Code、Plan 和 Ask modes 提供不同交互边界。[Exa: Windsurf modes](https://docs.windsurf.com/windsurf/cascade/modes)
- skills 使用 progressive disclosure 和 supporting files。[Exa: Windsurf skills](https://docs.windsurf.com/windsurf/cascade/skills)

差异：

- Windsurf 领先：IDE context、Arena comparison、worktree isolation、automatic memory 和 workflow UX。
- kxen 领先或差异化：明确 goal budget、multi-provider routing 和 local workflow runtime。
- kxen 必须补齐：并行结果对比、workspace isolation、mode 和 permission 统一、memory observability。

### 7.8 Cline

官方资料确认：

- checkpoint 使用 persistent shadow Git，并在工具操作后保存，恢复 code 时保留 conversation。[Exa: Cline checkpoints](https://docs.cline.bot/core-workflows/checkpoints)
- auto approve 按 command、browser 和其他 tool categories 配置。[Exa: Cline auto approve](https://docs.cline.bot/features/auto-approve)
- MCP 支持 stdio、HTTP 和 SSE。[Exa: Cline MCP](https://docs.cline.bot/mcp/mcp-overview)
- skills 使用 progressive disclosure。[Exa: Cline skills](https://docs.cline.bot/customization/skills)
- checkpoint hook 和 browser session 在源码中为独立 service。[gh_grep: Cline checkpoint service](https://github.com/cline/cline/blob/main/sdk/packages/core/src/session/session-versioning-service.ts) [gh_grep: Cline browser session](https://github.com/cline/cline/blob/main/apps/vscode/src/services/browser/BrowserSession.ts)

差异：

- Cline 领先：browser tool、per-category auto approval、checkpoint timing、IDE visibility、SDK。
- kxen 领先或差异化：team、cron、goal、MRM 和 voice。
- kxen 必须补齐：browser automation、checkpoint consistency、approval profile 和 SDK。

### 7.9 Roo Code

官方资料确认：

- skills 可为 mode 提供专用 instructions 和 supporting files。[Exa: Roo skills](https://docs.roocode.com/features/skills)
- checkpoint rewind 支持完整 history 恢复，并有 browser screenshot 和非破坏性 context management。[Exa: Roo update notes](https://docs.roocode.com/update-notes/v3.36)
- orchestrator mode 通过 new task 把工作委派到专门 context。[gh_grep: Roo boomerang tasks](https://github.com/RooCodeInc/Roo-Code/blob/main/apps/docs/docs/features/boomerang-tasks.mdx)

差异：

- Roo 领先：orchestrator mode、browser、checkpoint and history integration、IDE modes。
- kxen 领先或差异化：multi-provider credential pool、goal budgets、team persistent task file 和 voice。
- kxen 必须补齐：orchestration permission boundary、checkpoint preview、browser 和 history fidelity。

### 7.10 Conductor

官方资料确认：

- workspace 是 Git backed isolated unit，包含 branch、working tree、environment、commands 和 chats。[Exa: Conductor workspaces](https://www.conductor.build/docs/concepts/workspaces-and-branches)
- Git worktree 与 workspace、commands、chats 和 review flow 绑定。[Exa: Conductor worktrees](https://www.conductor.build/docs/concepts/git-worktrees)
- 支持 Claude Code、Codex、Cursor 和 OpenCode harness sessions。[Exa: Conductor harnesses](https://www.conductor.build/docs/reference/harnesses)
- 支持多 workspace 和同 workspace 多 agent，并有 review、merge 和 archive 流程。[Exa: Conductor parallel agents](https://www.conductor.build/docs/core/parallel-agents)
- 内建 diff viewer。[Exa: Conductor diff viewer](https://www.conductor.build/docs/reference/diff-viewer)

差异：

- Conductor 领先：workspace lifecycle、parallel task isolation、harness reuse、diff review 和 merge。
- kxen 领先或差异化：拥有自己的 provider、agent loop、goal、knowledge、voice 和 MCP runtime。
- kxen 必须补齐：把 session、branch、worktree、task、diff 和 review 合并为同一 execution unit。

### 7.11 Vibe Kanban

官方资料确认：

- issue、workspace 和 agent session 构成 planner 和 reviewer 外壳。[Exa: Vibe Kanban docs](https://www.vibekanban.com/docs)
- 支持并行 coding agents 和 Kanban issue flow。[Exa: Vibe Kanban getting started](https://vibekanban.com/docs/getting-started)
- 开源仓库包含 workspace、branch、terminal、dev server、diff review 和 inline comments。[Exa: Vibe Kanban GitHub](https://github.com/BloopAI/vibe-kanban)
- executor enum 包含 Claude Code、Gemini、Codex 和 OpenCode 等 harness。[gh_grep: Vibe executors](https://github.com/BloopAI/vibe-kanban/blob/main/crates/executors/src/executors/mod.rs)
- worktree manager 使用 per-repo creation lock。[gh_grep: Vibe worktree manager](https://github.com/BloopAI/vibe-kanban/blob/main/crates/worktree-manager/src/worktree_manager.rs)
- 官方首页当前说明项目 sunsetting，并转向 open source community maintenance。[Exa: Vibe Kanban status](https://vibekanban.com/)

差异：

- Vibe Kanban 领先：Kanban issue、attempt workspace、multi-harness、inline diff comment 和 execution view。
- kxen 领先或差异化：自建 agent runtime、goal、knowledge、MCP、LSP、voice 和 credential routing。
- kxen 必须补齐：task board、attempt lifecycle、per-repo worktree lock、diff review 和 failure history。

## 8. 按维度汇总全部差异

| 维度              | kxen 当前                                                  | 头部竞品基线                                                 | 差距判断 |
| ----------------- | ---------------------------------------------------------- | ------------------------------------------------------------ | -------- |
| Session isolation | 全局 model、extras、team dependencies                      | task 或 session 持有独立 context、permissions、workspace     | D        |
| Parallel work     | subagent、team、cron 可并行，但 workspace ownership 不统一 | worktree、VM 或 workspace 绑定 agent task                    | D        |
| Recovery          | JSONL、shadow Git、journal 各自恢复                        | checkpoint 与 conversation、tool 或 task lifecycle 绑定      | C        |
| Permissions       | resident tool profile 加 string safety rules               | per-tool allow、ask、deny，subagent 独立 permissions         | D        |
| Sandbox           | 无 OS sandbox 和 network policy                            | Codex、Gemini 和 cloud agents 提供 OS 或 container isolation | D        |
| MCP               | stdio tools only                                           | remote transport、OAuth、resources、prompts、roots、approval | C        |
| LSP               | Rust diagnostics only                                      | multi-language diagnostics 和 IDE semantic navigation        | D        |
| Knowledge         | 七类 OKF、globs、nearby rules、memory distill              | rules、skills、memory、plugins 或 extensions                 | B        |
| Provider          | 6 内置、custom protocols、多 CLI credentials               | OpenCode 和 Goose 更广，其他产品更受控                       | B        |
| Workflow          | QuickJS agent workflow 加 journal                          | Goose recipes、Windsurf workflows、Claude hooks 和 teams     | B        |
| Browser           | 无 browser tool                                            | Cursor cloud、Cline 和 Roo 内建 browser                      | FAIL     |
| Voice             | Apple speech 加 provider transcription                     | 本轮竞品资料未把 voice 列为统一基础能力                      | A        |
| UI                | 原生桌面 timeline、Dock、settings 和 workspace grid        | IDE native 或 multi-agent workspace review                   | B        |
| Observability     | activity、notifications、usage、diagnostics，多数内存      | task history、cloud logs、workspace lifecycle 和 review      | C        |
| SDK 和 headless   | 无                                                         | Claude、Codex、OpenCode、Goose、Cline 均有自动化 surface     | FAIL     |
| Release           | dmg，无 updater 和 signing identity                        | 成熟产品有签名、升级和发布通道                               | FAIL     |

## 9. 当前实现的差异化优势

以下结论只说明 kxen 当前源码已经实现的差异化，不宣称市场唯一：

- 四类外部 CLI credential discovery 加统一 provider store。[probe.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/auth/probe.rs#L26-L108)
- role routing、account rotation、provider and global limits 的 MRM 设计。当前主链缺少 acquire，但模型仍比单 provider client 更完整。[mrm.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/llm/mrm.rs#L1-L225)
- 8 状态 goal、token、turn、wall budget 和 blocker 计数。[goal.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/core/goal.rs#L1-L222)
- exact、semantic、stagnation 和 ABABAB 四类 loop detection。[loop_detect.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/loop_detect.rs#L1-L140)
- QuickJS workflow、agent dispatch、constraints 和 crash resume journal。[workflow.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/agent/workflow.rs#L1-L220)
- 七类知识、双 scope、四级 injection、dynamic globs 和 nearby rules。[render.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/knowledge/render.rs#L1-L160)
- 运行中和删除前双触发 memory distillation。[consolidate.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/knowledge/consolidate.rs#L1-L70)
- Apple speech partial 加 provider transcription fallback 的 PTT。[voice/mod.rs](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/voice/mod.rs#L41-L125)
- 单一 Tauri App 中整合 timeline、approval、goal、agents、tasks、worktrees、MCP、LSP、usage 和 notifications。

这些优势当前受到三类实现缺口限制：

- MRM 没有覆盖主 session 和 team request。
- OKF trust 没有覆盖所有执行型消费路径。
- 桌面整合使用大量进程级共享状态，不能保证 session 和 workspace 隔离。

## 10. 本轮自动验证结果

| 命令                                | 结果 | 实际证据                                                                    |
| ----------------------------------- | ---: | --------------------------------------------------------------------------- |
| `cargo fmt --check`                 | FAIL | examples 和 source 存在大量 rustfmt diff                                    |
| `cargo check --all-targets --quiet` | PASS | exit 0；LSP store 有 suspicious double reference clone warning              |
| `cargo test --quiet`                | PASS | 136 加 2 加 1 加 1 加 10，共 150 passed，0 failed                           |
| `pnpm check`                        | PASS | 122 个文件格式检查和 78 个文件 lint 检查通过                                |
| `pnpm test`                         | FAIL | 6 test files、23 tests 全部执行通过，但缺少 `jsdom` dependency，命令 exit 1 |
| `pnpm build`                        | PASS | 4565 modules transformed，build 完成；多个 chunk 超过 500KB                 |
| `marked.parse` raw HTML probe       | PASS | 输入 raw div 后输出仍为 raw div，证明 parser 不自动 sanitize                |

验证边界：

- 未启动 Tauri App。
- 未打开系统 UI。
- 未执行真实 provider request。
- 未连接真实 MCP server。
- 未启动 rust-analyzer 做真实 diagnostics。
- 未执行 microphone、Apple Speech、notification permission 和 OS approval。
- 未执行真实 multi-agent、cron、checkpoint、rewind 和 worktree destructive E2E。
- 所有上述 live 结果均为 UNKNOWN。

## 11. 文档与实现漂移

| 文档声明                                                                                              | 当前实现                                    | 结论             |
| ----------------------------------------------------------------------------------------------------- | ------------------------------------------- | ---------------- |
| [README.md](file:///Users/xiaobai/Code/SelfCode/kxen/README.md#L1-L22) 写 47 个 Rust tests            | 本轮 cargo test 为 150 个                   | FAIL，文档过期   |
| [PRD.md](file:///Users/xiaobai/Code/SelfCode/kxen/docs/PRD.md#L1-L45) 声明使用 Tauri updater          | Cargo dependency 和 App init 不存在 updater | FAIL             |
| [PRD.md](file:///Users/xiaobai/Code/SelfCode/kxen/docs/PRD.md#L1-L45) 未强调随机 local WebSocket port | 当前 UI 依赖随机 loopback WebSocket         | FAIL             |
| 旧分析把 MCP 判为 absent                                                                              | 当前 stdio MCP 已接入 agent loop            | FAIL，旧报告过期 |
| 旧分析把 LSP 描述为更完整 code intelligence                                                           | 当前只有 Rust diagnostics                   | FAIL             |
| UI 描述 Apple speech 为 offline zero cost                                                             | 没有强制 on-device recognition              | FAIL             |

证据：

- [README.md](file:///Users/xiaobai/Code/SelfCode/kxen/README.md#L1-L22)
- [PRD.md](file:///Users/xiaobai/Code/SelfCode/kxen/docs/PRD.md#L1-L45)
- [Cargo.toml](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/Cargo.toml)
- [voice objc bridge](file:///Users/xiaobai/Code/SelfCode/kxen/src-tauri/src/voice/objc.rs#L47-L100)

## 12. 明确修复优先级

### 12.1 第一阶段：先关闭安全和数据损坏路径

1. 为 local WebSocket 增加一次性 capability token、Origin validation、client identity 和 session stream ACL。
2. 所有 file-backed id 改为 backend generated opaque id，并在 canonical root 下验证。
3. 建立唯一 tool policy gateway。Exec、task、hook、worktree、MCP 和 context 全部经过相同 allow、ask、deny。
4. 增加 SSRF guard，禁止 loopback、private、link-local、metadata 和重定向逃逸。
5. Rewind 和 worktree removal 增加 dirty、active run、foreign session、approval 和 preview gate。
6. 为每个 session 增加 run mutex 和 JSONL writer lock，cron 只能 queue 或创建新 execution。

### 12.2 第二阶段：重构 runtime isolation

1. 引入 WorkspaceRuntime，持有 workdir、trust、hooks、MCP、LSP 和 team factory。
2. 引入 SessionRuntime，持有 model、account、extras、goal、pending queue、usage、draft 和 active run。
3. TeamManager 不再捕获启动时依赖，member 由所属 SessionRuntime 派生。
4. 主 session、subagent 和 teammate 全部通过同一个 MRM acquire contract。
5. Session 删除统一 cancel、join、unschedule、close team、flush transcript 和清理 memory state。

### 12.3 第三阶段：恢复和生态

1. 把 checkpoint 创建改为 awaited barrier，并记录 session id、message id、workspace head 和 dirty manifest。
2. Image、reasoning、exact tool arguments、full result reference 和 aggregate usage 进入 durable transcript。
3. MCP 增加 HTTP、SSE、OAuth、resources、prompts、roots、output cap 和 per-tool permission。
4. LSP 使用 language registry，至少覆盖 Rust、TypeScript、JavaScript、Python 和 Go，并加入 definition、references、hover 和 symbols。
5. 把 session、goal、task、branch、worktree、diff、review 和 execution history 合并为一个 workspace task model。
6. 增加 signed updater、headless runner、SDK surface 和真实 App E2E。

### 12.4 完成标准

满足以下条件后，才能把当前发布成熟度从 FAIL 改为 PASS：

- P0-01 到 P0-14 全部关闭，并有针对性 regression tests。
- Rust format、Rust check、Rust tests、frontend check、frontend tests 和 frontend build 全部 exit 0。
- 真实 App 完成 session isolation、workspace switch、cron collision、MCP permission、checkpoint rewind 和 crash recovery E2E。
- 完成 macOS sandbox policy、network policy、signing 和 updater 验收。
- 文档、配置 schema、UI 文案和 runtime 行为一致。

## 13. 最终判断

kxen 当前最强的部分是功能整合和产品构想：多 provider credential reuse、goal、team、workflow、knowledge、voice、MCP、LSP、checkpoint 和 worktree 已经出现在同一桌面产品中。

kxen 当前最弱的部分是边界一致性：同一能力在主 session、subagent、team、cron、hook、MCP 和 UI RPC 中没有共享同一权限、限流、持久化和生命周期规则。大量单点功能已经 PASS，但组合后的系统性质仍为 FAIL。

与 11 个同类产品相比，kxen 不缺更多功能入口。当前决定竞争力的工作是把已有功能收敛到 4 个统一契约：

- Workspace ownership
- Session isolation
- Tool permission and sandbox
- Durable execution and recovery

在这 4 个契约闭合前，继续增加 provider、tool、panel 或 agent role 会继续扩大现有风险面，不会提高生产成熟度。
