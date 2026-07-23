# LSP 诊断接入设计（G2）

- 状态：设计稿待确认，未开始实现
- 依据：docs/analysis/kxen-competitive-analysis-2026-07-23.md（生态层：LSP/代码智能全库零命中）

## 目标

让 agent 能拿到**编译器级诊断**（diagnostics：错误/警告，含行列与消息），首个 server 只接 `rust-analyzer`（本项目自食其果：Rust 代码占主体），ts-language-server 留扩展位。

## 范围与非目标

- 做：rust-analyzer 子进程生命周期管理、`textDocument/didOpen/didChange/publishDiagnostics`、诊断缓存与查询工具 `diagnostics(path?)`
- 不做：定义跳转/引用/重命名/hover（v1 只要诊断；这些是 v2 候选）、多语言并行 server、inlay hints

## 架构

```
src-tauri/src/lsp/
├── mod.rs        // LspManager：per-workspace 单实例 rust-analyzer
├── process.rs    // 子进程 spawn + Content-Length framing（LSP 标准头）
├── protocol.rs   // JSON-RPC 消息类型（initialize/didOpen/didChange/diagnostics）
└── store.rs      // 诊断缓存：path -> Vec<Diagnostic>（publishDiagnostics 驱动）
```

### 关键流程

1. 首个 diagnostics 请求时懒启动：spawn `rust-analyzer`，initialize（rootUri=workspace），didOpen 当前涉及文件
2. fs_tool write/edit 成功后：`didChange` 同步（tracker 已有全部涉及文件，挂同一调用点）
3. `publishDiagnostics` 通知更新 store；agent 工具 `diagnostics(path?)` 返回 store 快照（空则 "no diagnostics"）
4. 工具挂法：常驻小工具（core_tools），错误格式 `[E] path:line:col message (rust-analyzer)`

### 为什么不是全 LSP

诊断是 agent 自校验的最大杠杆（编译错直接喂回 loop 修），而定义跳转类能力 grep/glob 已覆盖 80%。单 server 单能力的实现量是全 LSP 的 1/5。

## 集成点

- `src-tauri/src/agent/tools_spec.rs`：core_tools 加 `diagnostics`
- `src-tauri/src/agent/agent_loop/execute.rs`：diagnostics arm -> LspManager
- `src-tauri/src/tools/fs_tool.rs`：write/edit 成功后通知（经 ctx.tracker 同源文件集）
- AppState：`lsp: LspManager`（per active_workspace 重建）

## 验证

- 单测：Content-Length framing 编解码、诊断缓存更新、空项目无诊断
- 集成：在测试 workdir 写一个有编译错的 main.rs，diagnostics 返回该行错误
