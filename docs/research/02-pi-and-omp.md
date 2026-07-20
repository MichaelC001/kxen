# Pi 与 OMP 深度调研

- 调研日期: 2026-07-20
- 说明: kxen 的技术底座与完整度标杆。Pi 提供哲学与 SDK，OMP 提供「电池齐全」的形态参照。

## 1. Pi（pi-mono）

### 基本信息

- 作者: Mario Zechner (@mariozechner / badlogic)
- 仓库: https://github.com/earendil-works/pi （2025-08 创建，原 badlogic/pi-mono，约 73k stars）
- 官网: https://pi.dev
- 许可证: MIT
- 语言: TypeScript 93.3%
- npm: `@mariozechner/pi-coding-agent`（旧）、`@earendil-works/pi-coding-agent`（新）
- 安装: `npm install -g @mariozechner/pi-coding-agent`（README 同时给出 `--ignore-scripts @earendil-works/pi-coding-agent`）

### monorepo 结构

| 包 | 职责 |
| --- | --- |
| `@earendil-works/pi-ai` | 统一多 provider LLM API（Anthropic、OpenAI、Google、xAI、Groq、Cerebras、OpenRouter、任意 OpenAI 兼容端点），流式、工具调用（TypeBox schema）、thinking、跨 provider 上下文交接、token / cost 统计 |
| `@earendil-works/pi-agent-core` | agent loop + `Agent` 类（状态管理、事件订阅、消息队列两种模式、附件、transport 抽象可直连或走代理） |
| `@earendil-works/pi-tui` | 终端 UI 库，differential rendering |
| `@earendil-works/pi-coding-agent` | CLI：session 管理（continue / resume / branching）、AGENTS.md 分层加载、slash command、自定义模型 / provider JSON、主题、headless JSON / RPC 模式 |

### 核心哲学（kxen 直接继承）

- 极小核心：默认只有 `read` / `write` / `edit` / `bash` 四个工具，system prompt + 工具定义 < 1000 token
- 不内置 sub-agents、plan mode、MCP：「Adapt Pi to your workflows, not the other way around」
- 扩展全靠自己写或装 package：extensions（TS 模块，可注册 tools / commands / 快捷键 / events / TUI 组件）、skills（渐进披露）、prompt templates、themes
- package 分发: `pi install npm:@foo/pi-tools` / `pi install git:github.com/user/repo@ref`，装到 `~/.pi/agent/npm|`git/` 或项目级 `.pi/`
- 内置 Claude Pro/Max 订阅 OAuth（作者博客功能列表确认）
- SDK 可嵌入: `import { AuthStorage, createAgentSession, ModelRegistry, SessionManager } from "@mariozechner/pi-coding-agent"`

### 与 kxen 需求相关的已验证 package（npm registry 2026-07-20 直查）

| 包 | 版本 | 说明 |
| --- | --- | --- |
| `pi-muselinn-harness` | 0.7.3 | Kimi Code 风格编排 harness：Swarm（并发 subagent + TUI）、Goal（生命周期）等；https://github.com/MuseLinn/pi-muselinn-harness |
| `@tintinweb/pi-subagents` | 0.14.2 | Claude Code 风格自治 sub-agent |
| `pi-subagents` | 0.35.1 | subagent 委派：链式、并行、TUI 澄清 |
| `@quintinshaw/pi-dynamic-workflows` | 3.3.0 | Claude Code 风格 dynamic workflows：可 fan-out 到上百个 subagent |
| `@gotgenes/pi-permission-system` | 20.9.0 | 权限强制执行 |
| `pi-codex-goal` | 0.1.38 | Codex 风格 goal 跟踪与续跑；https://github.com/fitchmultz/pi-codex-goal |
| `pi-agent-goal` | 2026.7.18 | 持久 `/goal` workflow：分支感知状态、进度工具；https://github.com/KristjanPikhof/Pi-Agent-Goal |
| `@narumitw/pi-goal` | 0.20.0 | 自治 `/goal` 完成 + 可选有序队列 |
| `pi-provider-kimi-code` | 0.6.7 | 在 Pi 里用 Kimi Code 且保留 Kimi 特性；https://github.com/Leechael/pi-provider-kimi-code |
| `pi-crew` | 0.9.44 | 多 agent 团队、workflow、worktree、异步编排 |
| `pi-lens` | 3.8.70 | LSP / linter / 格式化 / 类型检查反馈 |
| `pi-mcp-adapter` | 2.11.0 | MCP 适配 |

结论：kxen 要的每一块（Goal、Swarm、dynamic workflow、权限、Kimi provider）社区都已有可参考实现，但质量与整合度未知，定位是「参考 + 可借用」，不是「装上就用」。

## 2. OMP (oh-my-pi)

### 基本信息

- 作者: Can Bölük (can1357)
- 仓库: https://github.com/can1357/oh-my-pi （约 18.5k stars，2025-12-31 创建）
- 官网: https://omp.sh
- 许可证: MIT
- 运行要求: bun >= 1.3.14
- 安装: `curl -fsSL https://omp.sh/install | sh` 或 `bun install -g @oh-my-pi/pi-coding-agent`
- 最新 release: v17.0.5 (2026-07-18)
- 定位: Pi 的 fork，「A coding agent with the IDE wired in」

### 架构要点

- TypeScript 87.6% + Rust 核心约 55k 行（README 当前口径；早期文章写约 27k，持续增长）
- Rust crates: `pi-natives`（N-API 聚合）、`pi-shell`（内嵌 bash，brush fork + PTY，会话跨调用存活）、`pi-ast`（tree-sitter，50+ 语言）、`pi-iso`（隔离后端：APFS clone / btrfs / zfs reflink / overlayfs / projfs / rcopy）
- 热路径无 fork/exec：ripgrep、glob、find 全部进程内化，libuv 线程池执行；同一二进制跑 macOS / Linux / Windows（无 WSL）
- 四种入口: TUI、`omp -p` 单发、Node SDK、`omp --mode rpc` / `omp acp`
- 工具面: 32 内置工具、14 LSP ops、28 DAP ops、hash-anchored edit（hashline，拒绝 stale 编辑）、浏览器、Hindsight 记忆

### 模型路由（kxen 重点参考，来源: https://github.com/can1357/oh-my-pi/blob/main/docs/models.md 与 settings.md）

- 内置角色: `default`、`smol`、`slow`、`vision`、`plan`、`designer`、`commit`、`tiny`、`task`、`advisor`
- `modelRoles` 配置角色 -> 模型，支持 thinking 后缀（`:minimal` 到 `:max`）与 `@role` 别名
- `retry.fallbackChains`：按角色 / 精确模型 / `provider/*` 通配配置降级链；429 / 配额墙触发切换，`cooldown-expiry` 后恢复主模型
- round-robin credentials：同 provider 堆多个 key，会话亲和 + 按凭证退避
- path-scoped models：按仓库路径钉不同模型集
- context promotion：context overflow 时先升到配置的更大上下文模型再考虑 compaction
- auth 标签: `oauth`（anthropic / codex / gemini 等）、`plan`（coding-plan 订阅路由）、`local`
- 凭证解析顺序: runtime `--api-key` > models.yml 配置 key > 存储的 API key > 存储的 OAuth（多账号自动轮换）> 环境变量 / .env

### subagent（kxen 重点参考）

- `task` 工具派发 subagent，自动建隔离 worktree（APFS / btrfs / overlayfs）
- 返回 schema 校验的结构化结果，不是纯文本
- 支持并行 fan-out、channel 通信、orchestrate 多阶段流程
- subagent 自己的 fallback 链：agent 定义里多个 model pattern，第一个可解析的为主，其余为 fallback
- 另有 `@oh-my-pi/swarm-extension` 编排扩展包

## 3. 对 kxen 的启示清单

直接借鉴的设计：

- Pi 的扩展系统与 package 分发（kxen 兼容或复用其形态）
- OMP 的角色化模型路由 + fallback 链 + round-robin 凭证（kxen 在其上加全局并发 / 配额调度）
- OMP 的 subagent 隔离 worktree + typed 结果
- OMP 的 hashline 编辑（降低编辑失败率，间接省 token）
- OMP 的 auth 解析优先级链

kxen 要做而两者都没有的：

- 全局 Model Resource Manager（统一并发 / 速率 / 配额 / 预算调度，且对编排 AI 可见）
- Kimi 风格 Goal 生命周期与 Claude 风格 Dynamic Workflow 的深度整合（社区 package 有雏形，但没有与资源调度打通的实现）
