---
type: reference
description: 架构速览与模块边界（改代码前按需阅读）
---

# 架构速览

- 根即 crate 根：`src/{core,llm,auth,tools,agent}/` 单向依赖 agent -> tools -> llm -> auth -> core
- 通信：WS 双通道（/rpc 请求-响应 + /stream 订阅推送），端口启动随机分配经 window.eval 注入
- 调度：一切 LLM 调用与 subagent 派发经 `src/llm/mrm.rs` acquire/release
- 工具面：exec/read/edit/write/delete/task/goal/agent/workflow 常驻；其余经 tool_search 渐进披露
- 完整设计：`docs/rust/01-design.md`（唯一真相）
