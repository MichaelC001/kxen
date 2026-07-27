---
type: reference
description: 架构速览与模块边界，改代码前按需阅读
---

# 架构速览

- 应用前端位于 `src`，使用 SolidJS。
- Rust crate 位于 `src-tauri`，拥有 Runtime、持久化和系统能力。
- 前后端通过 WebSocket RPC 和 Stream 通信。
- 所有模型调用必须经过 `src-tauri/src/llm/mrm.rs`。
- 高频工具常驻，其他工具通过 Tool Search 渐进披露。
- 项目知识位于 `.agents` 和 `src-tauri/.agents/notes`。
- 官网和全部文档位于 `website`，使用 Cloudflare Nimbus。
- 当前权威 Runtime 文档: [https://kxen.ai/concepts/runtime](https://kxen.ai/concepts/runtime)
- 当前权威维护者文档: [https://kxen.ai/maintainers/repository](https://kxen.ai/maintainers/repository)
