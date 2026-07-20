# 开源 CLI Agent 全景与选型结论

- 调研日期: 2026-07-20
- 方法: exa 实搜 + 官方文档 / 仓库核实 + npm registry 直查
- 结论: 自研 kxen，以 Pi 系（pi-mono / OMP）为技术底座参考，TypeScript + Bun

## 1. 调研范围与排除项

需求前提：跳过所有模型官方 CLI 作为「使用工具」的选项（它们不支持导入其他模型），但官方 CLI 的源码与机制是重要参考。

| 工具 | 仓库 | 语言 | 许可证 | 备注 |
| --- | --- | --- | --- | --- |
| Claude Code | 闭源 | - | 专有 | workflow 机制参考，见 `research/04` |
| Codex CLI | https://github.com/openai/codex | Rust | Apache 2.0 | ChatGPT 订阅 OAuth 机制参考 |
| Grok Build | https://github.com/xai-org/grok-build | Rust（workspace 约 1M 行，explainx 报道） | Apache 2.0 | 2026-07-15/16 开源；不接受外部 PR、issues 关闭；从内部 monorepo 定期同步 |
| Kimi Code CLI | https://github.com/MoonshotAI/kimi-code | TypeScript 为主 | MIT | 内置 coder / explore / plan subagent；支持 ACP；Node.js 版（旧 Python 版已停维护） |

Grok Build 开源补充（来源: https://x.ai/news/grok-build-open-source 与 https://github.com/xai-org/grok-build ）：

- 二进制名 `xai-grok-pager`，官方安装后叫 `grok`
- 支持完全 local-first：自编译 + 指向自有 inference 端点（`~/.grok/config.toml`）
- 认证优先级：per-model api_key > 会话 token（`~/.grok/auth.json`）> `XAI_API_KEY`
- 内含移植自 Codex 的 tool handlers（THIRD_PARTY_NOTICES 有记录）
- 扩展面：skills、plugins、hooks、MCP servers、subagents

## 2. 第三方 Harness 候选（重点）

| 工具 | 仓库 / 站点 | 语言 | 多 provider | Sub-agent | 订阅复用 | 适合作为 |
| --- | --- | --- | --- | --- | --- | --- |
| OpenCode | https://github.com/anomalyco/opencode 、 https://opencode.ai | TypeScript | 75+ | 原生 primary + subagent，per-agent 可配 model 与 permission | 四个订阅全部可登录（Claude 有 ToS 风险） | 机制参考 |
| Pi | https://github.com/earendil-works/pi （原 badlogic/pi-mono）、 https://pi.dev | TypeScript (93%) | 强（pi-ai 统一 API） | 核心不内置，靠 package | 内置 Claude Pro/Max OAuth；其余走自定义 provider | 底座 / 哲学来源 |
| OMP (oh-my-pi) | https://github.com/can1357/oh-my-pi 、 https://omp.sh | TypeScript (87.6%) + Rust 核心（约 55k 行） | 40+ | task 工具 + 隔离 worktree + typed 结果 | oauth / plan / local 三类 auth 标签 | 完整度标杆 |
| Kilo Code | https://kilo.ai | TypeScript | 强 | 有 | 支持 SuperGrok OAuth 等 | 参考 |
| Hermes Agent | https://hermes-agent.nousresearch.com | Python | 极强 | 有 | 支持 SuperGrok / X Premium+ OAuth | 参考 |

OpenCode 机制要点（来源: https://opencode.ai/docs/agents/ ）：

- 内置两个 primary agent：Build（全工具）与 Plan（edit / bash 默认 ask），Tab 切换
- 内置 3 个 subagent，可用 `@` 手动调用；`subagent_depth` 控制嵌套深度（默认 1）
- `opencode.json` 或 markdown 文件（`~/.config/opencode/agents/`、`.opencode/agents/`）定义 agent
- 每个 agent 可独立指定 model、prompt、permission（edit / bash: allow / deny / ask）

## 3. 语言与执行性能对比

| 工具 | 语言 | 性能特征 |
| --- | --- | --- |
| Grok Build | Rust | 极高：启动快、内存低、并发强 |
| Codex CLI | Rust | 极高 |
| Goose | Rust | 极高，本地优先 |
| OMP | TS (Bun) + Rust N-API 核心 | 高：热路径无 fork/exec（ripgrep / glob / find / shell 进程内化） |
| Pi | TypeScript (Node/Bun) | 中：工具走外部进程；生态与热重载最好 |
| OpenCode | TypeScript | 中 |
| Kimi Code CLI | TypeScript (Node.js) | 中 |
| Aider | Python | 较弱：启动慢、内存高、并发弱 |

结论：纯 TS 方案的短板在工具热路径（搜索 / AST / 隔离），不在编排逻辑。kxen 采用「TS 为主 + 热点按需 Rust N-API」的渐进策略，与 OMP 已验证的路径一致（详见 `design/04-tech-stack.md`）。

## 4. 选型结论

- 不自研从零写：Pi 的 SDK（`createAgentSession` / `ModelRegistry` / `AuthStorage`）已被 Pi 与 OMP 两边验证可用
- 不直接 fork OMP：代码量大、意见强，裁剪成本高于在 Pi 底座上自建编排层
- kxen 路线：Pi 系底座 + 自研编排层（Goal / Dynamic Workflow / 全局资源调度）+ 参考 OMP 的完整度设计
- 架构方案对比详见 `design/01-architecture.md`

## 5. 信息来源

- https://pi.dev/
- https://mariozechner.at/posts/2025-11-30-pi-coding-agent/
- https://github.com/earendil-works/pi
- https://omp.sh/ 与 https://github.com/can1357/oh-my-pi
- https://opencode.ai/docs/agents/ 与 https://opencode.ai/docs/providers/
- https://x.ai/news/grok-build-open-source 与 https://github.com/xai-org/grok-build
- https://github.com/MoonshotAI/kimi-code
- https://www.kimi.com/code/docs/en/
