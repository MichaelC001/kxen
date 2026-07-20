# kxen 项目规范

kxen 是终端 Coding Agent Harness（纯 TypeScript + Bun）。本文件是给所有 agent 的项目铁律。

## 生态优先（违反即重写）

- 禁止重复造轮子。写任何 provider / auth / 工具 / TUI / 协议前，先查 pi（@earendil-works/pi-*）与 Bun 内置 API 是否已有
- pi-ai 内置 provider（anthropic / openai / openai-codex / xai / kimi-coding / moonshotai(-cn) / amazon-bedrock / google-vertex / zai-coding-cn 等）与 OAuth（anthropic / openai-codex / xai / device-code），一律复用，禁止另起端点
- TUI 一律用 pi 的 InteractiveMode / runPrintMode，禁止自造 readline 循环
- LSP 协议用 vscode-jsonrpc，禁止手写 JSON-RPC

## 技术栈

- Bun workspaces + catalog（不用 pnpm）；配置文件一律 TOML（markdown frontmatter 除外，那是 OKF 标准）
- TypeScript 7 strict；biome（tabs、单引号、分号）
- 测试用 bun test，与源码同目录 `*.test.ts`

## 常用命令

- `bun run check`：一键验证（biome + typecheck + test），提交前必过
- `bun run build` / `build:all`：二进制打包（本机 / 五平台）
- `bun run scripts/verify-providers.ts`：四订阅真实调用验证
- `bun run scripts/e2e-goal.ts` / `e2e-workflow.ts` / `e2e-multi.ts`：端到端验证

## 文档

- `docs/` 是设计唯一真相（prd / research / analysis / design），改设计先改文档
- `docs/sdlc/` 是任务级临时件（plan.md），已 gitignore
- 字符白名单：ASCII + 中文 + 全角标点；禁 emoji / em dash / smart quotes；箭头用 `->`；URL 带 https://
