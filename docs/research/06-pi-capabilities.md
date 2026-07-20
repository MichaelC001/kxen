# pi 官方能力全清单（2026-07-20 盘点）

- 来源: pi-mono 包内 docs/（25 篇）、npm @earendil-works、GitHub earendil-works 组织、https://pi.dev
- 用途: kxen 基于 pi 增强，此表是「能用现成就不自造」的对照清单

## 1. monorepo 包（npm @earendil-works，均 MIT）

| 包 | 版本 | 能力 | kxen 使用状态 |
| --- | --- | --- | --- |
| `pi-ai` | 0.80.10 | 统一多 provider LLM API：40+ 内置 provider（含变体）、流式、工具调用、thinking 档位、OAuth（anthropic / openai-codex / xai / github-copilot / device-code）、跨 provider 交接、token/cost 统计、模型目录自动发现 | 全量使用 |
| `pi-agent-core` | 0.80.10 | agent loop、状态管理、事件流、消息队列（steer / followUp 双模式）、附件、transport 抽象（直连或代理） | 全量使用 |
| `pi-coding-agent` | 0.80.10 | CLI 本体 + SDK（createAgentSession / ModelRegistry / SessionManager / main） | 全量使用（CLI 委托其 main()） |
| `pi-tui` | 0.80.10 | TUI 组件库：differential rendering、编辑器（自动补全 / 模糊文件搜索 / 拖拽图片 / 多行粘贴）、markdown 渲染 | 用（InteractiveMode 承载） |
| `pi-web-ui` | 0.75.3 | Web 聊天 UI 组件（浏览器侧） | 未用（kxen 是终端工具，后续 web 面板可用） |
| `pi-radius` | 0.1.7 | Radius provider、web tools、skills | 未用 |
| `gondolin` (+runner) | 0.12.0 | Alpine Linux 沙箱（不可信代码执行，文件系统 / 网络受控） | 未用（沙箱后置，首选它而不是自研） |

## 2. pi-coding-agent 产品级能力（docs/ 25 篇对应）

| 能力 | 说明 | kxen 状态 |
| --- | --- | --- |
| 四种运行模式 | interactive TUI / print（-p）/ JSON event stream / RPC | CLI 已穿透（委托 main()） |
| SDK | createAgentSession 等嵌入接口 | 用 |
| Sessions | 存储、分支（tree）、resume、命名、HTML 导出 | pi 内置，kxen 直接用 |
| Compaction & branch summary | 内置压缩（阈值触发）与分支摘要 | 用（kxen 的 clearing 是其补充，不重复） |
| Skills | 按需加载的能力包（SKILL.md 渐进披露） | 可用未接（kxen skills 走同一机制即可） |
| Prompt templates | markdown 斜杠命令（`~/.pi/agent/prompts`、`.pi/prompts`） | 用（kxen prompts 写在自己 agentDir） |
| Extensions | TS 扩展：工具 / 命令 / 快捷键 / 事件 / UI 组件 | 用（inline extensionFactories + agentDir/extensions 发现） |
| Themes | 终端主题（内置 + 自定义，live reload） | 未配置（默认主题） |
| Packages | 打包分发 extensions/skills/prompts/themes（npm / git，`pi install`） | 未用（kxen 扩展后续应打包成 pi package） |
| 自定义 provider / 模型 | models.json、registerProvider、OpenAI 兼容端点 | 用（kimi-coding 内置无需自定义） |
| Settings | settings.json 分层（全局 / 项目 / 运行时） | pi 内置 |
| Keybindings | 可配置键位 | pi 内置 |
| Security | project trust（目录信任） | pi 内置 |
| Shell aliases | 启动时捕获用户 shell 别名 | pi 内置 |
| Context files | AGENTS.md 分层加载（全局 -> 项目 -> 子目录） | pi 内置（kxen 的 .agents/ 是其超集） |
| tmux 集成 | 多窗格派生 pi 实例 | 未用 |
| Containerization | 容器化运行指南 | 未用 |
| Windows / Termux | 平台支持 | 未验证（kxen 目前 macOS） |
| OAuth 登录流 | /login 交互式登录（Claude Pro/Max 等） | 用（kxen 另加官方 CLI 凭证导入） |
| 成本 / token 统计 | 全程 usage 与 cost 追踪 | 用（进 MRM 预算） |
| 消息队列 | 流式中 steer（打断）/ followUp（排队） | 用（subagent steering 基于此） |

## 3. 官方关联项目（GitHub earendil-works）

| 项目 | 说明 | kxen 可用性 |
| --- | --- | --- |
| pi（monorepo） | 上述全部 | 底座 |
| gondolin | Linux microvm / Alpine 沙箱（TS 控制面） | 沙箱首选（M5+） |
| pi-review | 官方 review 扩展 | 可装作 review 能力参考 |
| pi-chat | 聊天相关 | 参考 |
| pi-tutorial | 交互式教程模式 | 可作 kxen 新手引导参考 |
| clipboard | Rust 剪贴板（macOS / Windows / Linux 图文） | 需要时引入 |
| absurd | durability 实验 | 参考其持久化思路 |

## 4. 立即该做（对照出的缺口）

1. kxen 的扩展（/write-goal、/goal、/workflow）打包成 pi package（npm/git 分发），而不是只内联在 cli 里
2. 沙箱选型直接评估 gondolin，不自研
3. themes / keybindings 提供 kxen 默认配置而不是另做系统
4. pi-review / pi-tutorial 评估后作为官方扩展示范接入
