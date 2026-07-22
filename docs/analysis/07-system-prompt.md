# 分析: 系统提示词的定义与注入

- 日期: 2026-07-20
- 前提: kxen 方向明确为只做 coding 场景，prompt 只覆盖软件工程任务，不做通用助手
- 方法: exa 实搜 + 逆向仓库 + grok-build 源码 + Claude Code prompt 原文（2888 行）结构分析

## 1. 逆向资源清单（已核实）

| 仓库                                                             | 规模                                        | 价值                                                                                                                                                                                     |
| ---------------------------------------------------------------- | ------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| https://github.com/asgeirtj/system_prompts_leaks                 | 约 59k stars，日更                          | 最新最全：Claude Code 按模型分档 prompt（Opus / Sonnet / Haiku 各版本）、injected reminders、MCP server prompts、deferred tools、bundled skills、Codex 按模型分档与 plan mode / personas |
| https://github.com/x1xhlol/system-prompts-and-models-of-ai-tools | 约 142k stars                               | 覆盖面最广（30+ 工具），偏 coding 工具                                                                                                                                                   |
| https://github.com/elder-plinius/CL4R1T4S                        | 约 45k stars                                | 多 vendor                                                                                                                                                                                |
| https://github.com/YeeKal/leaked-system-prompts                  | 有在线站 https://leaked-system-prompts.com/ | 可搜索、可对比                                                                                                                                                                           |
| https://github.com/xai-org/grok-prompts                          | xAI 官方                                    | Grok 官方 prompt                                                                                                                                                                         |
| https://docs.anthropic.com/en/release-notes/system-prompts       | Anthropic 官方                              | Claude 官方发布版                                                                                                                                                                        |

结论: 以 asgeirtj 仓为主索引，grok-build 开源源码为权威（不用逆向）。

## 2. 两个流派的结构对比

### Claude Code（巨石单文件流派，2888 行）

组装顺序（原文章节）：

1. 身份 + 授权安全声明（静态）
2. System：输出渲染、权限模式说明、`<system-reminder>` 标签语义、hooks 语义、自动压缩告知
3. Doing tasks：任务类型、最小改动原则、注释政策、UI 验证要求
4. Executing actions with care：可逆性 / 爆炸半径、危险操作清单
5. Using your tools：专用工具优先、并行调用规则
6. Tone and style / Text output
7. Session-specific guidance
8. auto memory：记忆类型 / 不存什么 / 怎么存 / 何时读
9. Environment、Scratchpad Directory、Context management
10. `# Session context`：gitStatus、claudeMd、userEmail、currentDate（动态注入点）
11. `# Agents`：subagent 定义全文注入
12. `# Skills`：skill 清单
13. `# Tools`：每个工具的完整文档（Bash 工具的 git commit / PR 指南就占 100+ 行）

特点: 工具文档全文内联进 system prompt；按模型分档（Sonnet / Haiku / Opus 措辞不同）；injected reminders 作为运行时独立注入层。

### grok-build（模板渲染流派，开源可读）

- MiniJinja 模板，运行参数插值；模板 XOR 混淆存放（仅防 strings 扫描，非安全边界），用时解密，`Zeroizing` 擦除
- 工具名永不硬编码: `${{ tools.by_kind.* }}` 渲染期解析，工具改名 / 换命名空间 prompt 不用改；`${%- if tools.by_kind.X %}` 条件段可整体隐藏不存在工具的章节
- 分模板: base（主 agent）、apply_patch（Codex 移植）、subagent（短模板）；内置 subagent profile: EXPLORE / GENERAL_PURPOSE / PLAN
- 压缩后系统侧缩为极简 stub: `You are an AI coding agent... Your main goal is to complete the user's request, denoted within the <user_query> tag`，让摘要承载任务状态
- reminders 策略化: TodoNudge（3 轮没用 todo_write 就提醒，间隔至少 5 轮）、TodoGate（纯文本收尾但还有未完成 todo 时强制再来一轮，默认关）

### Pi（极简流派）

system prompt + 工具定义合计 < 1000 token，只注入 AGENTS.md 分层文件；其余一切靠 skills / extensions 按需加载。

### Codex

按模型分文件（gpt-5.x 各版本独立 prompt）+ plan mode 专用 prompt + personas（friendly / pragmatic）可叠加。

## 3. 注入机制全景（kxen 可用的六个注入层）

| 层           | 内容                                                | 缓存属性               | 来源实践                                                    |
| ------------ | --------------------------------------------------- | ---------------------- | ----------------------------------------------------------- |
| 静态核       | 身份、任务原则、安全、风格                          | 稳定，前缀缓存         | 各家共同                                                    |
| 工具使用政策 | 粒度、并行、纠偏规则                                | 稳定                   | Claude Code                                                 |
| 会话上下文块 | env / cwd / git status / date                       | 每会话变，放静态核之后 | Claude Code `# Session context`                             |
| 项目规则     | AGENTS.md 分层（全局 -> 项目 -> 子目录按需）        | 项目级稳定             | Claude Code / Pi / grok-build                               |
| 运行时提醒   | `<system-reminder>` 附加在工具输出后，事件驱动      | 不进前缀               | grok-build reminders / Claude injected reminders / OMP TTSR |
| 迟绑定       | 请求末尾追加合成 user message（记忆、MRM 状态快照） | 不进前缀               | DCP late-binding                                            |

缓存铁律: 任何跨轮稳定的内容必须形成字节级一致的前缀；动态内容只能追加不能插入（Anthropic tool search 的 append-not-swap 原则）。

## 4. 关键工程发现（直接影响 kxen 设计）

1. 工具文档全文进 prompt 是 Claude Code 的选择，代价是 2888 行；kxen 走「核心工具文档内联 + 非常驻工具 tool search 渐进披露」，与 T8 一致
2. 工具名间接引用（grok-build 模板语法）是多模型 harness 的必需品：不同 provider / 不同工具子集渲染不同 prompt，源码一处维护
3. 按模型分档 prompt（Claude / Codex 都这么做）: kxen 的角色系统天然需要「base + role overlay + model-family overlay」三层
4. subagent 用独立短模板，绝不继承主 prompt 全文；工具白名单在 schema 层过滤（OpenDev），看不到的工具就不会误用
5. 压缩后缩 system prompt（grok-build COMPACT_SYSTEM_PROMPT）+ 重注入项目规则文件（Claude Code），两者结合
6. TodoNudge / TodoGate 类「行为提醒器」是提醒框架的落地样例，kxen 可推广为「goal 进度提醒」「budget 水位提醒」
7. 身份段写法由方向决定：kxen 只做 coding，身份段应显式限定软件工程场景，非 coding 请求礼貌拒绝或转述，避免通用助手化带来的行为漂移

## 5. kxen prompt 组装设计（P1-P11）

| #   | 决策                                                                                                                                                                              | 依据                                  |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- |
| P1  | Section-based composer：段有优先级与条件，按「静态核 -> 工具政策 -> 会话块 -> 项目规则 -> 清单 -> 工具文档」排序，字节级稳定前缀                                                  | OpenDev composer + 缓存铁律           |
| P2  | 模板渲染：工具名 / 角色名 / 阈值全部模板变量插值，条件段隐藏缺席工具，禁止硬编码                                                                                                  | grok-build                            |
| P3  | 三层变体：base + role overlay（thinking / execution 等）+ model-family overlay（Claude / GPT / Grok / Kimi 措辞差异）                                                             | Claude Code / Codex 按模型分档        |
| P4  | subagent 独立短模板（explore / plan / execute / review），工具白名单 schema 级过滤                                                                                                | grok-build + OpenDev                  |
| P5  | 压缩后 system stub + 重注入项目规则文件                                                                                                                                           | grok-build + Claude Code              |
| P6  | 提醒框架内置两种：per-tool（空文件、截断告知）与 cross-cutting（goal 进度、budget 水位、todo 久未更新），全部走 `<system-reminder>`                                               | grok-build reminders + analysis/01 C6 |
| P7  | MRM 状态快照以迟绑定 user message 注入 planning / thinking 角色，不进静态前缀                                                                                                     | DCP late-binding + analysis/03        |
| P8  | 不可信内容（网页、issue、外部文本）XML fencing，身份只进属性                                                                                                                      | DCP                                   |
| P9  | 身份段显式限定 coding 场景；非 coding 请求的处理策略写入静态核                                                                                                                    | 方向决策                              |
| P10 | prompt 文本全部文件化进仓库（模板 + 每角色 overlay），变更走 git 评审，运行期可从 `/tmp` dump 实际发出的完整 prompt 供调试                                                        | Cahciua 请求 dump + 工程化原则        |
| P11 | 静态核不内置内容类拒绝清单（逆向 / 破解 / 外挂等 dual-use 话题不做提示词风控）；prompt 只陈述能力事实与灾难操作的机制边界（规则集见 design/05，由执行层拦截），安全靠机制不靠说教 | prd 3.7 安全模型                      |

## 6. 反模式

- 不把 AGENTS.md 内容复制进 prompt 模板（规则文件由加载层注入，模板保持纯净）
- 不在系统 prompt 里写 provider 名 / 模型名的硬编码（走模板变量）
- 不让 subagent 继承主 prompt（短模板 + schema 过滤）
- 不把动态状态（时间、git status、MRM 水位）插入静态段中间（只能尾部追加）
