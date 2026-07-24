// MCP server 状态与手动重启（设置页高级区面板）。
import { client } from "./client";

export interface McpServerStatus {
  name: string;
  status: string; // "running" | "down" | "needs_auth"（待 OAuth 交互授权）
  transport: string; // "stdio" | "http" | "sse"
  url: string | null; // remote server 的 URL；stdio 为 null
  tools: number;
  resources: number;
  prompts: string[]; // prompt 名称列表
}

export function mcpStatus(): Promise<McpServerStatus[]> {
  return client.rpc("mcp.status");
}

export function mcpRestart(name: string): Promise<void> {
  return client.rpc("mcp.restart", { name });
}

export interface McpAuthBegin {
  authorize_url: string; // 授权 URL（opened=false 时展示给用户手动复制）
  opened: boolean; // 后端是否成功拉起浏览器
}

// 发起 OAuth 交互授权：后端后台等回调换 token，完成后自动重连并经通知中心告知
export function mcpAuth(name: string): Promise<McpAuthBegin> {
  return client.rpc("mcp.auth", { name });
}
