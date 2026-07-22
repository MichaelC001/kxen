# 分析: 工具面设计与命令质量

- 日期: 2026-07-20
- 方法: exa 实搜 + 官方文档 + 源码直读（grok-build bash 工具 4809 行源码、Codex execpolicy）
- 结论: 工具面是 harness 的第二核心。差异不在「有没有 bash」，而在于：命令粒度引导、误用纠偏、输出减载、专用工具不变量、渐进披露

## 1. 问题定义

bash 给模型的是无限能力，给 harness 的却是一个不透明字符串：同一个形状覆盖所有动作，harness 无法 gate、无法渲染、无法审计、无法判断能否并行（来源: https://github.com/Piebald-AI/claude-code-system-prompts 的 skill-agent-design-patterns）。

由此产生四类失败：

- 粒度失败: 把 4-5 个命令用 `&&` / 管道 / heredoc 堆进一次调用，任何一段失败则全部信息丢失，且难以并行
- 误用失败: 用 `cat` 读文件（丢失行号与 read-before-edit 追踪）、用 `sed -i` 改文件（绕过唯一性校验）、结尾加 `&` 后台（harness 失去生命周期管理）
- 输出失败: 一次 dump 几万行进上下文
- 安全失败: 危险命令无法按动作类型分级审批

## 2. 命令粒度：各家怎么引导

| 手段          | 做法                                                                                                                | 来源                            |
| ------------- | ------------------------------------------------------------------------------------------------------------------- | ------------------------------- |
| 并行调用      | 同一轮发多个独立 Bash 调用，harness 并发执行；一次拿齐 git status + diff + log                                      | Claude Code 工具文档与 playbook |
| 依赖链        | 有依赖的命令用 `&&` 串联，失败短路                                                                                  | 同上                            |
| 后台任务      | `run_in_background` / `background: true` 立即返回 task_id，完成时通知，明确「不要 poll」                            | Claude Code 与 grok-build 一致  |
| 超时策略      | grok-build: 超默认超时自动转后台（auto_background_on_timeout）而不是杀死；强杀时杀进程组（SIGTERM -> SIGKILL 升级） | grok-build 源码                 |
| 尾部 `&` 纠偏 | grok-build 检测到结尾 `&` 直接拒绝，返回针对性纠正文案（「删掉 &，改用 background 参数」），按场景出四种文案        | grok-build 源码                 |
| shell 方言    | grok-build 模板化工具描述：`shell_uses_semicolon` 时明确「不支持 &&，用 ; 串联」；Windows / pwsh 差异单独处理       | grok-build 源码                 |

要点: 粒度不是靠提示词祈祷，而是「harness 检测误用 -> 拒绝 -> 给出可执行的纠正路径」，让模型下一次调用就对了。

## 3. 输出减载：各家怎么截断

| 机制                   | 细节                                                                                                                                       | 来源                     |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------ |
| first/last 截断 + 落盘 | grok-build: 超 max_output_bytes 时保留头尾，附注 `[truncated: showing first/last X of Y - full output at: <path>]`，全文写入文件供按需再读 | grok-build 源码          |
| 尾部保留压限           | OpenCode V2: 压缩保留尾部里的工具输出统一压到 2000 字符                                                                                    | opencode compaction 文档 |
| 序列化预算             | OMP snapcompact: 工具结果 head+tail 2000 字符（0.6 head 比）、参数值 500、单调用 2000，工具输出灰色弱显                                    | OMP 源码                 |
| 无用标记               | OMP: 标记 `useless` 的工具结果连调用一起整对跳过，不进档案                                                                                 | OMP 源码                 |
| MCP 输出截断           | grok-build: 独立 mcp_truncate 模块处理 MCP 工具输出                                                                                        | grok-build 源码          |
| 中间结果不进上下文     | PTC（programmatic tool calling）: 脚本编排多次调用，只有最终输出回模型                                                                     | Anthropic 文档           |

## 4. 专用工具 > bash：不变量清单

把动作从 bash 提升为专用工具的收益（综合 Piebald 文档与各实现）：

- 门控: 按可逆性分级审批（`send_email` 好 gate，`bash -c "curl -X POST ..."` 没法 gate）
- 新鲜度: edit 工具可拒绝「读之后文件已变」的写（staleness）；OMP hashline 用内容 hash 锚定直接消灭 stale 编辑
- 并行调度: 只读工具（grep / glob / read）可标记 parallel-safe；同样动作走 bash 就只能串行
- 审计渲染: 类型化参数可渲染 diff、可进权限规则（如 `Bash(npm run *)` 这类模式匹配）
- grok-build 的反手操作: 当 shell 没有 unix 工具集时，在 bash 工具描述里明确「grep / head / tail / sed / awk / find 不可用，请用专用工具」，把模型从 shell 习惯里硬拉出来

经验法则（Anthropic / Piebald 一致）: bash 先行拿广度；一旦某动作需要 gate / 渲染 / 审计 / 并行，就提升为专用工具。

## 5. 渐进披露：不把 schema 塞进上下文

| 机制                   | 细节                                                                                                                                                                          | 来源                                                      |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------- |
| tool search            | 工具 schema 默认延迟加载，按搜索追加而不是替换（保 prompt cache）                                                                                                             | Anthropic / Claude Code                                   |
| search_tool + use_tool | grok-build: search_tool 发现 MCP 工具，use_tool 按名分发；误用原生工具名时返回纠正错误「直接调用它」                                                                          | grok-build 源码                                           |
| skills 懒加载          | 描述常驻、正文按需读；压缩后已调用 skill 有 5000/个、25000 总量上限                                                                                                           | Claude Code                                               |
| CLI + README           | Pi 哲学: 能力做成带 README 的 CLI，模型用 bash 按需读 README，按 token 计费为零（直到需要）                                                                                   | https://mariozechner.at/posts/2025-11-30-pi-coding-agent/ |
| repo map               | Aider: tree-sitter 抽定义/引用 -> PageRank 个性化排序（chat 文件 50x、提及标识符 10x、引用数开平方防霸榜）-> 二分搜索装进 token 预算（默认 1024，无文件时 x8；行截 100 字符） | aider.chat 文档与源码                                     |

## 6. 权限与安全分层（Codex 标杆）

来源: https://openai-codex.mintlify.app/advanced/exec-policies 与 openai/codex 源码

- execpolicy 用 Starlark 写 `prefix_rule(pattern, decision, justification, match, not_match)`，decision 三档: allow / prompt / forbidden，多规则命中取最严（forbidden > prompt > allow）
- match / not_match 样例即测试，加载时校验
- 全局 `~/.codex/execpolicy.rules` + 项目 `.codex/execpolicy.rules` 双层
- 与沙箱（macOS Seatbelt / Linux Landlock+seccomp / Windows restricted token）正交：策略决定「能不能跑」，沙箱决定「跑出界会怎样」
- 未命中规则的兜底决策按 approval policy 与沙箱类型推导；危险命令在 prompts 关闭时是 Forbidden 而不是静默放行
- 用户批准一次后，系统可提议 execpolicy amendment（把该类命令固化为 allow），减少重复打扰

## 7. kxen 设计决策

| #   | 决策                                                                                                                                   | 依据                                         |
| --- | -------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------- |
| T1  | bash 工具描述模板化（运行时参数插值: 超时、后台能力、shell 方言）                                                                      | grok-build                                   |
| T2  | 误用纠偏器: 尾部 `&`、轮询后台任务、`cat` 读文件、`sed -i` 改文件 -> 拒绝 + 针对性纠正文案                                             | grok-build 纠偏 + Claude Code 专用工具不变量 |
| T3  | 输出 first/last 截断 + 全文落盘，附重新读取路径                                                                                        | grok-build                                   |
| T4  | 独立调用并行执行、依赖调用 `&&` 串联，写进工具描述并给例子                                                                             | Claude Code playbook                         |
| T5  | 超时不杀而转后台（可配），强杀杀进程组                                                                                                 | grok-build                                   |
| T6  | 只读工具集（grep / glob / read / ls）标记 parallel-safe，写工具走串行与权限                                                            | Piebald 调度原则                             |
| T7  | edit 工具带 staleness 拒绝；中期评估 hashline 等价物                                                                                   | OMP                                          |
| T8  | 工具搜索: 全部非常驻工具（MCP、低频专用工具）走 search -> append schema -> 保缓存                                                      | Anthropic tool search + grok-build use_tool  |
| T9  | repo map 作为 codebase 定向层: tree-sitter + PageRank + token 预算二分，默认 1k token 基准、随上下文占用自适应伸缩（无文件在聊时放大） | Aider                                        |
| T10 | 权限规则引擎: Starlark 或等价 DSL，allow / prompt / forbidden 三档取最严，支持 match / not_match 自测，项目级文件可进 git              | Codex execpolicy                             |
| T11 | 批准后固化: 用户批准一次即提议写规则，减少重复打扰                                                                                     | Codex amendment                              |
| T12 | 工具 eval 管线: 每个工具配评估任务集，描述与 schema 的修改必须过 eval                                                                  | Anthropic writing-tools-for-agents           |
