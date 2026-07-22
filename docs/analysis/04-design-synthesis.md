# kxen 设计总纲: 优点收纳矩阵

- 日期: 2026-07-20
- 定位: 全部调研（`docs/research/`、`docs/analysis/01-03`、`05-07`）收敛为一份选型定稿
- 目标: 优雅（统一抽象，不堆砌）、高效（token 与成本优先）、准确（纠偏与验证内置）

## 1. 收纳原则

- 每个维度选一个主方案（生产验证最充分的）+ 至多一个备选，不并列三四个
- 主方案有源码或官方文档依据；备选标注引入时机
- 与多订阅混用冲突的方案降级（例: 单模型假设的固定阈值）
- 任何机制必须服务于三条主线之一：context 效率、调用质量、调度确定性

## 2. 维度矩阵

| 维度           | 最佳实践来源                            | kxen 选择                                                                                                                                                                                                                 | 备选（引入时机）                               |
| -------------- | --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------- |
| Context 回收   | Anthropic 三原语 + OpenCode 两级        | tool-result clearing 常开 -> 结构化摘要 -> 溢出恢复（每步一次）                                                                                                                                                           | snapcompact 等价物（M5，需 vision + 原生渲染） |
| 压缩阈值       | OMP 公式 + OpenCode PR #10123 需求      | contextWindow - max(15%, reserve)，按模型分档可配                                                                                                                                                                         | -                                              |
| 防死循环       | Claude Code + OpenCode                  | 连续 N 次失败熔断 + 压缩后 min_messages 闸门                                                                                                                                                                              | -                                              |
| 摘要结构       | OpenCode SUMMARY_TEMPLATE               | 六段式: objective / decisions / progress / blockers / next / files                                                                                                                                                        | -                                              |
| 摘要模型       | OpenCode / OMP                          | tiny 角色（tools off，上限 4096 output），记账                                                                                                                                                                            | -                                              |
| 记忆           | Claude Code + OMP Hindsight             | 项目规则文件（压缩后重注入）+ agent 自维护索引记忆（200 行上限 + 主题文件）                                                                                                                                               | 离线两阶段管线（M5，用 smol 角色）             |
| 事件提醒       | grok-build reminders + OpenDev 论文     | system-reminder 框架: per-tool + cross-cutting，附加于工具输出，可存活压缩                                                                                                                                                | TTSR 流中断规则（M5，OMP）                     |
| Codebase 定向  | Aider + Claude Code                     | 双轨: repo map（tree-sitter + PageRank + token 二分，默认 1k token）做全局定向；grep / glob / read 做按需精查                                                                                                             | import 依赖估计增强（aider-ce 思路，M5）       |
| 命令粒度       | grok-build + Claude Code                | 独立调用并行同轮、依赖 `&&` 串联、后台返回 task_id 通知                                                                                                                                                                   | -                                              |
| 误用纠偏       | grok-build                              | 尾部 `&` / 轮询后台 / `cat` 读文件 / `sed -i` 改文件 -> 拒绝 + 针对性纠正文案                                                                                                                                             | -                                              |
| 超时处理       | grok-build                              | 超时转后台（可配），强杀杀进程组 SIGTERM -> SIGKILL                                                                                                                                                                       | -                                              |
| 输出减载       | grok-build + OpenCode + OMP             | first/last 截断 + 全文落盘附路径；压缩保留区统一 2000 字符压限；工具结果可标 useless                                                                                                                                      | -                                              |
| 专用工具不变量 | Claude Code + Piebald + OMP             | edit 带 staleness 拒绝与唯一性校验；只读工具标 parallel-safe                                                                                                                                                              | hashline 等价物（M5）                          |
| 渐进披露       | Anthropic tool search + grok-build + Pi | 非常驻工具（MCP / 低频）走 search -> append schema；skills 懒加载；能力优先做成带 README 的 CLI                                                                                                                           | -                                              |
| 权限规则       | Codex execpolicy                        | DSL 三档 allow / prompt / forbidden 取最严；match / not_match 自测；项目级规则文件进 git；批准后提议固化                                                                                                                  | -                                              |
| 安全边界       | 用户决策（prd 3.7）                     | 灾难操作机器拒绝（毁系统 / 毁用户目录 / 删 git 仓库 / 毁数据与基础设施，完整规则集见 design/05），执行层三重拦截（命令解析 + 路径守卫 + 审计）；内容类话题（逆向 / 破解 / 外挂等）不做提示词级风控，prompt 不内置拒绝清单 | -                                              |
| 模式权限       | OpenCode + OpenDev                      | plan / build 双模式: plan 只读 + 研究 subagent，write 工具从 schema 移除而不只是 deny                                                                                                                                     | -                                              |
| 检查点回滚     | Gemini CLI + Claude Code                | 文件修改前 shadow git 快照 + `/rewind` 恢复（含会话状态）                                                                                                                                                                 | -                                              |
| Subagent       | OMP + kimi-code                         | typed 结构化结果 + worktree 隔离 + 前后台双模式 + swarm 批量派发                                                                                                                                                          | CoW 隔离（APFS / overlayfs，M5）               |
| 子代理上下文   | DCP (Cahciua)                           | TR 以 provider 中立 IR 持久化 + composeContext 确定性规则集（见 analysis/05），多模型 fallback 不炸历史                                                                                                                   | probe gate（M4，fan-out 前置裁判）             |
| 高层编排       | Claude Code + kimi-code                 | Dynamic Workflow（脚本化编排 + 缓存恢复）与 Goal（contract + 状态机 + 预算）打通，见 design/03                                                                                                                            | -                                              |
| 模型调度       | 见 analysis/03                          | MRM: bucket 预检 + AIMD + 优先级队列 + 熔断 + 角色 fallback                                                                                                                                                               | -                                              |
| 可观测         | Claude Code + OpenHands                 | `/context` 分类统计 + 全事件流（每次回收 / 降级 / 纠偏可见）                                                                                                                                                              | -                                              |
| 内存工程       | CC / OpenCode 事故教训                  | 进程级内存预算 + 三层分离 + 低阈值落盘 + 有界队列 + 订阅生命周期绑定 + telemetry（analysis/06 E1-E8）                                                                                                                     | -                                              |
| 图表渲染       | grok-build                              | mermaid 纯 Rust 渲染（dagre / mermaid-to-svg + resvg，N-API），kitty -> OSC1337 -> ASCII 分档，源 hash 缓存                                                                                                               | `AGENT_GRAPHICS` 拦截约定（随 bash 工具一起）  |

## 3. 三条主线的映射

高效（token 与成本）：

- clearing 常开 + 结构化摘要 + tiny 模型摘要 -> 长会话成本有界（OpenHands 实测冷凝后每轮成本降约一半且性能不降）
- repo map 默认 1k token 替代漫无目的的整文件读取
- 输出减载三件套（first/last、2000 压限、useless 标记）
- 执行类角色路由到限额宽的订阅，思考类角色留给强模型 -> 同样的预算做更多轮

准确（少犯错，错了能纠正）：

- 误用纠偏让错误调用当场变成教学信号，而不是污染上下文
- edit staleness 拒绝与唯一性校验消灭整类编辑失败
- goal completion 走可执行验证，不走模型自评
- workflow 支持对抗复核（独立 review 角色复核 execution 产出）

优雅（一个抽象管到底）：

- 所有模型调用过 MRM 唯一入口；所有编排（goal / workflow / subagent）复用同一 acquire 协议
- 工具、规则、记忆、workflow 全部文件化（可读、可 diff、可进 git）
- 扩展沿用 Pi 形态（TS extension + package），核心保持小

## 4. 反模式（明确不做）

- 不把 MCP 作为默认扩展路径：优先 CLI + README 与原生扩展，MCP 走 search -> use_tool 渐进披露（Pi 哲学 + grok-build 实现）
- 不做云同步与账号体系：记忆、会话、规则全在本地文件
- 不做 IDE：TUI 为主，ACP / RPC 后置
- 不做沙箱：权限规则（execpolicy + 路径守卫）即全部执行边界
- 不为单一模型写死任何阈值、提示词或工具集（多模型是前提，不是适配）
- 不做提示词级内容风控：硬性保护为灾难操作规则集（F1-F5，见 design/05）且全在执行层；安全边界靠机制不靠说教

## 5. 与里程碑的对齐（与 design/01 一致）

- M0-M1: T1-T6（工具面基础）、MRM 骨架、plan/build 模式、内存三层分离与预算
- M2: subagent typed 结果 + worktree 隔离 + IR 存储与 compose 规则、swarm
- M3: goal 引擎、检查点回滚
- M4: workflow runtime、保存命令、probe gate
- M5: snapcompact、TTSR、hashline、CoW 隔离、离线记忆管线、mermaid、发布管线
