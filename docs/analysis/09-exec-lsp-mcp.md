# 分析: exec 多 shell 工具、LSP 与 MCP 自动探测

- 日期: 2026-07-20
- 定位: kxen 是 code 类 agent，命令执行、代码智能、外部工具发现三件事必须是一等公民
- 依据: grok-build bash 源码、Claude Code / Copilot CLI / OMP / Serena / agent-lsp 公开资料、VS Code MCP 文档（均为 2026-07-20 实搜）

## 1. exec 工具：显式 shell 类型，让模型自我理解

### 问题

bash / zsh / fish / cmd / powershell 语法差异巨大：变量赋值与导出、数组下标（zsh 从 1 开始）、`&&` 支持与否、重定向、heredoc、Windows 上没有 unix 工具集。统一叫 `bash` 时模型按 bash 习惯生成命令，在非 bash 环境直接失败，失败输出污染上下文。

### 业界对应做法（已核实）

- grok-build：工具描述按 `shell_uses_semicolon` / `has_unix_utilities` / `is_windows` 模板条件渲染，明确告诉模型「`&&` 不支持用 `;`」「grep / sed / awk 不可用请用专用工具」；PowerShell 5.1 与 pwsh 7+ 对尾部 `&` 的处理分别写文案
- Codex：execpolicy 按命令 token 匹配，与 shell 无关的一层
- Claude Code：启动时 source 用户 shell rc 捕获 alias / function，但命令仍按 bash 方言执行

### kxen exec 设计（X1-X6）

| # | 决策 | 依据 |
| --- | --- | --- |
| X1 | 工具签名 `exec(type, path, command, timeout?, background?)`，`type` 枚举 `zsh / bash / fish / cmd / powershell`，必填无默认 | 类型显式化迫使模型先想环境再写命令 |
| X2 | 工具描述按 type 模板化渲染方言卡片：该 shell 的变量 / 数组 / 串联 / 重定向规则、可用工具集、与 `&&` / heredoc 的兼容性 | grok-build 模板条件段 |
| X3 | 方言校验器：按 type 做静态检查（fish 里的 `export`、cmd 里的 `&&` 旧版不支持、zsh 数组下标假设等），命中即拒绝 + 纠正文案，与 T2 纠偏器合并实现 | grok-build 纠偏模式 |
| X4 | 默认 type 探测：会话启动探测用户 login shell 并作为推荐值写入环境块；模型仍可显式指定别的 type（如脚本必须 bash 时） | Claude Code shell rc 捕获的启发 |
| X5 | 会话状态按 (type, path) 维度维护：cwd 持久规则与 shell 解耦，换 type 即新会话上下文 | Claude Code cwd 持久 + grok-build 会话化 shell |
| X6 | Windows 路径单独处理：cmd / powershell 的命令解析器独立实现，不套 unix 规则 | grok-build / Codex Windows 处理 |

## 2. LSP：code 类 agent 的原生能力，不走外包

### 价值证据（已核实）

- 精度：`findReferences` 返回 23 个真实调用点 vs grep 500+ 噪声；局部变量与模块级同名符号 grep 无法区分
- 速度：约 50ms vs 递归文本搜索数十秒
- 闭环：每次 edit 后语言服务器立刻报类型错误 / 未定义符号，agent 在用户运行前就能修（edit -> 诊断 -> 修的紧反馈环）

### 各家集成对比

| 方案 | 代表 | 形态 | 评价 |
| --- | --- | --- | --- |
| 原生内置 | OMP（14 LSP ops） | 进程内 LSP 客户端 | 最快最可控，kxen 方向 |
| 插件注册 | Claude Code 2.0.74+（marketplace 插件，如 Piebald-AI/claude-code-lsps） | 外部 LS 二进制注册 | 依赖插件生态 |
| 配置文件注册 | GitHub Copilot CLI（`~/.copilot/lsp-config.json` + `.github/lsp.json`） | JSON 声明 command + 文件扩展名 | 简单但手动 |
| MCP 外包 | Serena（40+ 语言，符号级工具 + 记忆）、agent-lsp（65 工具、常驻 daemon 热索引、speculative edit） | MCP server | 重，且与「MCP 渐进披露」原则一致时才引入 |

### kxen LSP 设计（L1-L6）

| # | 决策 |
| --- | --- |
| L1 | 原生 LSP 客户端内置（不走 MCP）：definition / references / hover / documentSymbol / workspaceSymbol / rename / diagnostics / codeAction 八 ops 起步 |
| L2 | auto-detect：扫描项目标记文件（package.json / tsconfig / Cargo.toml / go.mod / pyproject / *.sln 等）推断语言集，探测 PATH 上的 language server（vtsls / typescript-language-server / gopls / rust-analyzer / pyright 等内置映射表，可配置扩展），懒启动、按文件类型路由会话 |
| L3 | edit / write 工具执行后自动拉 diagnostics，作为 `<system-reminder>` 附在工具结果后（与提醒框架 C6 打通），模型立刻看到自己改出来的错误 |
| L4 | rename / 大重构走 `workspace/willRenameFiles` 等语义操作，保证引用同步（OMP 做法） |
| L5 | 索引期保护：大型项目后台索引，索引未就绪时 LSP 工具返回降级结果而不是阻塞；索引进度进 statusline |
| L6 | 与 repo map（T9）互补：repo map 做全局定向，LSP 做精确查询；不写文件的项目不启动 LS |

## 3. MCP：auto-detect + 渐进披露

### 配置位置全景（已核实）

| 工具 | 项目级位置 | 用户级位置 |
| --- | --- | --- |
| Claude Code | `.claude/mcp.json`（亦有 `.mcp.json`） | `~/.claude/` |
| Cursor | `.cursor/mcp.json` | `~/.cursor/mcp.json` |
| VS Code | `.vscode/mcp.json` | 用户 profile `mcp.json` |
| GitHub Copilot | `.github/mcp.json`（及 `.vscode/`、`.vs/`） | `~/.copilot/` |
| Codex | `.codex/config.toml`（TOML） | `~/.codex/` |

VS Code 还有 `chat.mcp.discovery.enabled` 自动从 Claude Desktop 等其他应用发现配置（有过「误用 Cursor 配置」的串扰案例，说明发现必须显式可控）。

### kxen MCP 设计（MCP1-MCP5）

| # | 决策 |
| --- | --- |
| MCP1 | auto-detect：启动时扫描上表项目级与用户级位置，统一导入为候选清单，TUI `/mcp` 面板可见来源 |
| MCP2 | 发现不等于启动：候选 server 默认不启动，用户在面板启用（approval），项目级 server 需要目录信任 |
| MCP3 | 冲突规则：项目级优先于用户级；同名 server 就近原则；可显式 disable |
| MCP4 | 工具走渐进披露：启用的 server 不进全量 schema，走 search -> append（T8），与 grok-build 的 search_tool / use_tool 同型 |
| MCP5 | 首选配置格式用通用 `mcp.json`（JSON），`.codex/config.toml` 做只读导入；kxen 自己只写 `mcp.json` 与面板状态 |

## 4. 反模式

- 不给 exec 设默认 shell 类型还号称支持多 shell（默认即 bash 方言泛滥的根源）
- 不把 LSP 走 MCP 外包给第三方 server 当主路径（原生客户端是 code 类 agent 的嫡系能力）
- 不让 auto-detect 到的 MCP server 自启动（投毒面；发现 -> 面板 -> 显式启用）
