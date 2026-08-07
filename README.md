# kxen

macOS Apple Silicon 原生 Coding Agent Harness。kxen 使用 Tauri 2、Rust、SolidJS 和 QuickJS，把多 Provider 模型、目标、工作流、Agent Teams、本地工具、安全审批和长期知识组织在一个桌面 Runtime 中。

官网与权威文档: [https://kxen.ai](https://kxen.ai)

当前状态: 开发预览。v0.0.1 已通过完整发布链公开: 全部 CI 门禁、Developer ID 签名、Apple 公证(App 与 DMG)、产物逐字节校验和 GitHub Release 自动更新通道均为 `PASS`。外部发布治理已就位: `release` environment 的 8 个签名 secret 全部为 environment secret 且 deployment branch policy 仅允许 `main`，`v*` tag ruleset 禁止更新和删除已发布 tag，Actions policy 已开启 full-length commit SHA pinning。GitHub Immutable Releases 无公开 API 可查，状态为 `UNKNOWN`，需仓库设置中确认。

## 主要能力

- Workspace、Session 和 Composer。
- 持久 pending queue、原子续跑与 Session JSONL/Queue 存储恢复。
- 多 Provider、多账号、角色路由和 MRM。
- Goal、Subagent、Dynamic Workflow 和 Agent Teams。
- 文件、Shell、Web Fetch、Web Search、Browser、MCP 和 LSP 工具。
- Knowledge、Rules、Skills 和 Memory。
- Checkpoint、Rewind 和 Worktree。
- Voice、Schedule、Usage、通知和诊断。
- 执行层 Safety、Approval 和可恢复删除。

## 开发应用

```bash
pnpm install
pnpm tauri:dev
```

## 验证应用

```bash
pnpm check
pnpm typecheck
pnpm test
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

`pnpm check` 是 CI 权威前端静态门禁，依次执行 format、lint 和 TypeScript strict typecheck；`pnpm typecheck` 可单独复现类型检查。

## 开发官网

```bash
cd website
pnpm install
pnpm dev
```

## 验证官网

```bash
cd website
pnpm check
```

官网使用 Cloudflare Nimbus。产品介绍和产品文档统一保存在 `website` package 中，开发调研、实现计划和内部 QA 不进入产品站，根 `docs` 目录不再使用。
