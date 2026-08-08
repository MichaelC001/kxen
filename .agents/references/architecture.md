---
type: reference
description: 架构速览与模块边界，改代码前按需阅读
---

# 架构速览

- 应用前端位于 `src`，使用 SolidJS。
- Cargo workspace 位于仓库根：`crates/kxen-core`（lib `kxen_core`，全部产品逻辑：Runtime、持久化、系统能力）、`crates/kxen-cli`（无头 server bin `kxen`）、`src-tauri`（`kxen-gui` Tauri 桌面壳 crate）。
- 前后端通过同一个内嵌 `/ws` WebSocket 端点的 RPC 和 Stream 通信，桌面 webview 与浏览器是平等客户端。
- RPC handler、`request_schema` 和生产前端调用必须保持完全对称；当前契约由 `crates/kxen-core/tests/rpc_contract.rs` 静态门禁维护。
- 所有模型调用必须经过 `crates/kxen-core/src/llm/mrm.rs`。
- 受保护的对外 HTTP 必须使用 `tools::net_guard` 的 guarded 或显式 loopback client builder，不继承环境代理；Browser 的 Chrome 流量必须经过进程内代理。
- Session 和 PendingQueue 在 PostCommit 持久性不确定时 fail closed，只能通过 `recovery.inspect` -> `recovery.repair|clear` 的精确验证链解除阻塞。
- 高频工具常驻，其他工具通过 Tool Search 渐进披露。
- 项目知识位于 `.agents`（含 `.agents/notes`）。
- 官网和全部产品文档位于 `website`，使用 Cloudflare Nimbus。
- 当前权威 Runtime 文档: [https://kxen.ai/concepts/runtime](https://kxen.ai/concepts/runtime)
- 当前权威维护者文档: 根目录 `CONTRIBUTING.md`。
