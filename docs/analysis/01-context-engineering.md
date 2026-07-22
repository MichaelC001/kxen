# 分析: Context 工程

- 日期: 2026-07-20
- 方法: exa 实搜 + 官方文档 + 源码直读（GitHub API / raw）
- 结论: context 工程是 harness 的第一核心。kxen 采用「分层回收 + 结构化摘要 + 事件驱动提醒 + 文件化记忆」的组合方案，全部机制选型均已在生产级工具中验证

## 1. 统一框架（Anthropic 官方）

来源: https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents 与 https://platform.claude.com/cookbook/tool-use-context-engineering-context-engineering-tools

- 指导原则: 找到「最大化目标结果概率的最小高信号 token 集合」；context 是工作集，不是存储层
- 三类原语，各治一种病：
  - compaction: 整窗操作，对话接近上限时蒸馏为摘要，有损但覆盖所有增长源
  - tool-result clearing: 子窗操作，把旧的、可重新获取的 tool_result 替换为占位符，保留 tool_use 记录，最安全最轻量
  - memory: 把信息移出窗口持久化，跨压缩、跨会话存活
- API 级实现参考: compact（默认 150K 触发）、clear_tool_uses（默认 100K，保留最近 3 次）、memory tool

## 2. 各 Harness 机制对比（已核实）

| 维度       | Claude Code                                                                                                                 | OMP                                                                                                                                       | OpenCode                                                 | Grok Build                                | OpenDev (论文)           |
| ---------- | --------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- | ----------------------------------------- | ------------------------ |
| 触发阈值   | 约 95%（可调），有效窗口 = context - 摘要预留                                                                               | contextWindow - max(15%, 16384)                                                                                                           | V2: 预检估算 > limit - max(output, 20000 buffer)         | 约 85% 或 /compact                        | 五阶段渐进压缩           |
| 摘要方式   | 模型摘要（circuit breaker 防连续失败）                                                                                      | 4 种策略: context-full / handoff / shake / snapcompact                                                                                    | 会话模型、tools off、最多 4096 output                    | full-replace 结构化摘要，同模型 tools off | 渐进丢弃旧观察           |
| 尾部保留   | 未公开                                                                                                                      | keepRecentTokens 20000                                                                                                                    | keep.tokens 8000（工具输出压到 2000 字符）               | 原始 goal 种子保留                        | -                        |
| 溢出恢复   | 连续失败后停手并报错                                                                                                        | overflow / incomplete(stopReason=length) / mid-turn / idle 四类自动路径                                                                   | 每步仅恢复一次，二次溢出报错                             | -                                         | -                        |
| 轻量清理   | tool clearing（snip）                                                                                                       | shake（无 LLM 直接切除重工具结果）                                                                                                        | V1 prune（保护最近 2 turn + 40K，最少清 20K；V2 未实现） | -                                         | -                        |
| 记忆       | CLAUDE.md 分层 + auto memory（MEMORY.md 200 行 / 25KB）                                                                     | Hindsight: retain / recall / reflect 工具 + 两阶段离线管线（extraction 用 default 角色、consolidation 用 smol 角色），注入上限 5000 token | 无内置                                                   | AGENTS.md + memory hints                  | playbook 策略记忆        |
| 压缩后存活 | 项目根 CLAUDE.md / 无 scope 规则 / MEMORY.md 重注入；path-scope 规则与嵌套 CLAUDE.md 丢失；skills capped 5000/个 25000 总量 | TTSR 注入规则存活；branch summary                                                                                                         | 明确标注「历史上下文，不是新指令」                       | -                                         | reminders 事件驱动重注入 |
| 可观测     | /context 分类统计                                                                                                           | /context 显示预估节省                                                                                                                     | -                                                        | -                                         | -                        |

Claude Code 内部机制来源说明: 部分来自社区重建源码（https://github.com/claude-code-best/claude-code ），包括有效窗口计算、连续失败 circuit breaker、session memory compaction 实验、CONTEXT_COLLAPSE（90% commit / 95% blocking）特性，仅作方向参考，非官方确认行为。

## 3. 突破性机制: OMP snapcompact

来源: https://github.com/can1357/oh-my-pi/blob/main/docs/compaction.md 与 packages/snapcompact

- 用确定性本地渲染取代 LLM 摘要: 被丢弃的历史序列化后渲染成像素字体 PNG，vision 模型直接读回，近乎逐字
- 成本: 1568px 帧约 40k 字符（约 10k token 文本）按 Anthropic 像素公式只计 3279 image token，约为文本输入价的 1/3；零模型调用、零延迟（除渲染）、零网络
- 按模型定制 shape: Claude 高分辨率线（Opus 4.7+）用 1932px 帧顶到 4784 visual-token 上限；Gemini 用 2048px（固定 1120 token/图）；GPT 用 1568px（面积计费）；Kimi/GLM 用 1568px（处理器超过 1792px 会降采样）
- 序列化时就做减载: 工具结果 head+tail 截断（默认 2000 字符、0.6 head 比）、工具参数截断（值 500 / 调用 2000）、工具输出用灰色弱显、标记为 useless 的工具结果整对跳过
- 布局: 时间轴两端保留原文（foveated HQ/LQ/HQ），帧数上限 80，超出丢最旧
- 限制: 需要 vision 模型，否则回退 context-full

评价: 这是目前唯一把「压缩保真度」和「零摘要成本」同时拿到的方案，也是多模型 harness 的天然搭档（shape 按模型路由）。

## 4. 事件驱动提醒（anti attention-decay）

- Grok Build: reminders 模块把 `<system-reminder>` 附加到工具输出后；分 per-tool（如空文件警告）与 cross-cutting（LSP 诊断、skill 发现、任务完成）两类，注册在 registry 上统一收集（源码: crates/codegen/xai-grok-tools/src/reminders/）
- OMP time-travel stream rules: 正则命中即中断流、把规则作为 system reminder 注入、从断点重试；纠正发生在当下且不占每轮 context；注入可存活 compaction；`/omfg` 可从翻车现场自动起草规则
- OpenDev 论文: event-driven system reminders 专门对治长会话中的 instruction fade-out；条件化 prompt composition 把 system prompt 拆成按优先级加载的段落，区分可缓存 / 不可缓存段

## 5. kxen 设计决策

| #   | 决策                                                                                                                             | 依据                                                          |
| --- | -------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| C1  | 三层回收: tool-result clearing（默认开）-> 结构化摘要 -> 溢出恢复（每步一次）                                                    | Anthropic 原语框架 + OpenCode 两级实践                        |
| C2  | 阈值可配且按模型分档: 默认 contextWindow - max(15%, reserve)，弱模型 / 长上下文退化模型可设更低（30-50%）                        | OMP 公式 + OpenCode PR #10123 反映的强需求                    |
| C3  | 摘要结构化: objective / decisions / progress / blockers / next / files 固定六段                                                  | OpenCode SUMMARY_TEMPLATE 与 Claude 实践                      |
| C4  | 摘要走 tiny 角色模型（tools off，上限 4096 output），记账进预算                                                                  | OpenCode + OMP 角色复用                                       |
| C5  | 中期引入 snapcompact 等价物作为可选策略（依赖 vision 模型与原生渲染，放第二阶段）                                                | OMP 已验证收益                                                |
| C6  | system reminder 框架: per-tool + cross-cutting 两类，附加在工具输出后，可存活 compaction                                         | Grok Build reminders + OMP TTSR + OpenDev                     |
| C7  | 记忆双轨: 项目规则文件（AGENTS.md 分层，压缩后重注入）+ agent 自维护记忆（索引文件 200 行上限 + 主题文件，离线管线用 smol 角色） | Claude Code + OMP Hindsight                                   |
| C8  | 可观测: `/context` 分类统计 + 每次回收事件进事件流                                                                               | 两家共同实践                                                  |
| C9  | 防死循环: 连续 N 次压缩失败熔断；压缩后需至少 M 条消息才允许再次压缩                                                             | Claude Code circuit breaker + OpenCode PR #10123 min_messages |
