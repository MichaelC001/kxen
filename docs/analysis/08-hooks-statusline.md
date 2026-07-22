# 分析: Hooks 与 Statusline

- 日期: 2026-07-20
- 定位: kxen 对 hooks 与 statusline 全面管控，默认收敛、可选开启
- 主要依据: https://code.claude.com/docs/en/hooks （30 事件参考）、 https://code.claude.com/docs/en/statusline.md

## 1. Claude Code hooks 机制全解（对标蓝本）

事件集（30 个，按阶段分）：

| 阶段 | 事件                                                                                                        |
| ---- | ----------------------------------------------------------------------------------------------------------- |
| 会话 | SessionStart / Setup / SessionEnd / InstructionsLoaded / ConfigChange                                       |
| 提示 | UserPromptSubmit / UserPromptExpansion / MessageDisplay                                                     |
| 工具 | PreToolUse / PostToolUse / PostToolUseFailure / PostToolBatch / PermissionRequest / PermissionDenied        |
| 编排 | SubagentStart / SubagentStop / TeammateIdle / TaskCreated / TaskCompleted / WorktreeCreate / WorktreeRemove |
| 回合 | Stop / StopFailure（细分 rate_limit / overloaded / billing_error 等）                                       |
| 其他 | Notification / FileChanged / CwdChanged / PreCompact / PostCompact / Elicitation                            |

通信协议（command hooks）：

- stdin 收 JSON 上下文；exit 0 = 放行（stdout 解析 JSON 输出）；exit 2 = 阻断（stderr 反馈给模型）；其他 = 非阻断错误
- JSON 输出字段: `continue` / `stopReason` / `suppressOutput` / `systemMessage` / `hookSpecificOutput`（permissionDecision: allow / deny / ask / defer，updatedInput 改写参数，updatedToolOutput 改写结果，additionalContext 注入上下文）
- HTTP hooks: POST 同样 JSON；非 2xx 一律非阻断（防止 hook 服务挂掉卡死会话）
- 过滤: `matcher`（工具名 / 竖线列表 / 正则）+ `if` 字段按工具参数过滤（`Bash(git *)`、`Edit(*.ts)`，v2.1.85+）
- 优先级: deny > defer > ask > allow；PreToolUse 先于权限模式检查，其 deny 连 bypassPermissions 都不可绕过
- 安全边界: deny 规则永远压过 hook 的 allow（hook 不能放宽，只能收紧或放行正常流程）

## 2. Claude Code statusline 机制全解

- 配置: `statusLine.type = "command"`，脚本路径或内联命令；`padding`、`refreshInterval`（秒，事件驱动之外的周期刷新）
- 协议: 每次 tick 往脚本 stdin 打一份会话 JSON，stdout 即渲染内容
- JSON schema 亮点: model / workspace（current_dir、git_worktree、repo）/ cost（USD、行数）/ context_window（used_percentage、current_usage 缓存细分）/ rate_limits（five_hour、seven_day，含 resets_at）/ agent / worktree / vim mode / effort / thinking / pr
- `subagentStatusLine`: 每个 subagent 行的自定义渲染，输入 tasks 数组，输出 `{"id", "content"}` JSONL，未覆盖的行用默认
- `/statusline` 命令用自然语言生成脚本；社区有 ccstatusline 等成品
- 工程经验: 脚本必须容错（字段缺失降级不崩）、快（单 jq 调用）、无网络无密钥、git 状态做 5s 缓存

## 3. kxen hooks 设计（全面管控 + 可选开启）

### 事件集

对齐 CC 核心集（会话 / 提示 / 工具 / 回合 / 压缩），并加 kxen 特有编排事件：

- GoalCreated / GoalPaused / GoalResumed / GoalCompleted / GoalBlocked
- WorkflowStart / WorkflowPhaseStart / WorkflowPhaseEnd / WorkflowEnd
- SubagentSpawn / SubagentComplete（含 typed result 摘要）
- MRMDegraded（provider 熔断 / 降级发生）/ BudgetWarning（水位 80% / 95%）

### 管控模型（与「全面管控但可选」对齐）

- 统一注册表：所有 hooks 来源（内置 / 全局配置 / 项目配置 / 扩展包）都进一张表，TUI `/hooks` 面板可视化查看、逐个开关、查看最近触发记录
- 默认收敛: 不带任何用户 hook；内置 hook 默认关闭（如 PostToolUse 自动 format），用户显式开
- 项目级 hooks 需要目录信任才激活（防恶意仓库投毒）；`disableAllHooks` 主开关一键全停
- 三种形态: command（stdin JSON + exit code，兼容 CC 语义）、HTTP（非 2xx 非阻断）、内置 TS 函数（性能敏感路径，免 fork；权限高于 command）
- matcher + if 参数过滤、deny > defer > ask > allow 优先级、hook 的 allow 不可放宽 deny 规则，全部沿用 CC 语义以降低迁移成本

### 与权限 / 规则引擎的关系

- hooks 是「可编程的扩展点」，execpolicy 是「机械的规则」：灾难防护（design/05）只走 execpolicy + 路径守卫，不走 hooks（hook 脚本可被项目投毒，不能当安全边界）
- hooks 可以收紧（deny / ask），不能放宽规则引擎的 forbidden

## 4. kxen statusline 设计

- 默认内置（不开脚本也有）：当前模型 + 角色、context 水位、MRM 各 provider 状态灯（健康 / 冷却 / 熔断）、goal 状态、预算水位、worktree / 分支
- 自定义协议：兼容 CC 的 JSON schema 并做超集（加 roles、mrm、goal、workflow、budgets 字段），现有 ccstatusline 类脚本可直接复用
- `subagentStatusLine` 对齐 CC 语义；swarm 场景每行显示 agent 的模型 / 角色 / token / 状态
- refreshInterval 默认关（事件驱动足够）；MRM 状态变化本身就是事件源
- 脚本容错要求写进文档：字段缺失必须降级、禁止网络与密钥、单次执行 <100ms

## 5. 反模式

- 不把安全决策放 hooks（脚本可投毒；安全只在 execpolicy + 路径守卫）
- 不默认开启任何内置 hook（全面管控意味着显式 opt-in）
- 不让 hook 输出直接进模型上下文而不标注（统一走 `<system-reminder>` 包裹，与提醒框架一致）
