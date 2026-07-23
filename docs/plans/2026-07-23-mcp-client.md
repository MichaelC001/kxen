# MCP client 接入设计（G1）

- 状态：设计稿待确认，未开始实现
- 依据：docs/analysis/kxen-competitive-analysis-2026-07-23.md（P0 差距 #1：全库无 MCP，9 家竞品全部支持）

## 目标

kxen 作为 MCP **client**（不做 server）：连接用户配置的 MCP servers（stdio 进程为主，SSE/HTTP 为辅），把其 tools 挂进 agent loop 的工具面，设置页可视化管理。

## 范围与非目标

- 做：stdio transport（spawn 子进程，JSON-RPC 2.0 over stdin/stdout）、`initialize` / `tools/list` / `tools/call`、`.mcp.json` 双 scope 配置（项目 `.mcp.json` + 用户 `~/.config/kxen/mcp.json`）、超时与崩溃重启、工具名前缀隔离（`mcp__<server>__<tool>`）
- 不做：MCP server 端、resources/prompts 订阅、sampling、OAuth（v1 只支持无认证 stdio + Bearer header 的 HTTP）

## 架构

```
src-tauri/src/mcp/
├── mod.rs        // McpManager：server 生命周期（start/stop/list/restart）
├── config.rs     // .mcp.json 解析（双 scope merge，项目覆盖用户）
├── transport.rs  // stdio 子进程（tokio process + 行分隔 JSON-RPC framing）
├── client.rs     // JSON-RPC client：initialize handshake + 请求/通知分发
└── tools.rs      // tools/list 缓存 + call 转发（deferred_tools 动态挂出）
```

### 关键流程

1. AppState 启动：`McpManager::from_configs(user, project)`，对每个 server spawn 子进程，`initialize` 握手（协议版本 2024-11-05 + clientInfo kxen），`tools/list` 缓存到内存
2. 工具面：`tools_spec::deferred_tools()` 之外加 `mcp::tool_defs()`——每个 MCP tool 展开为一个 ToolDefinition（名字 `mcp__server__tool`，schema 原样转发）
3. 调用：execute.rs 加前缀匹配分支 `name.starts_with("mcp__")` → McpManager.call(server, tool, args) → JSON-RPC `tools/call` → result.content 拼文本回传
4. 进程守护：call 失败（Eof/超时 30s）标记 server down，下次调用前 lazy restart（指数退避，对齐 llm/retry.rs）

### 配置形态（.mcp.json，与 Claude Code 兼容）

```json
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
    },
    "remote": {
      "type": "http",
      "url": "https://mcp.example.com/sse",
      "headers": { "Authorization": "Bearer ..." }
    }
  }
}
```

## 安全

- server 命令经 safety::evaluate_shell_command 拦截（F1-F5 全规则）+ 项目 .mcp.json 走信任门（未信任项目的 server 不启动，对齐 core/trust.rs 既有语义）
- tools/call 的参数原样转发不评估（server 自身职责域）；call 结果按 ToolResult 通道正常入时间线

## UI

- 设置页「高级」区加 MCP 管理面板：server 列表（状态点 running/down/starting）、手动 restart、查看 tools 清单、config 文件位置

## 里程碑

1. transport + client + initialize/tools/list 单测（mock stdio server 用 zsh cat echo 模拟）
2. McpManager + 双 scope config + 信任门
3. execute.rs 挂接 + tools/call 端到端
4. 设置页面板

## 验证

- 单测：framing 解析、config merge、工具名前缀展开、崩溃重启退避
- 集成：本地起一个 echo MCP server（10 行 shell），agent 调用 mcp__echo__say 返回原文
