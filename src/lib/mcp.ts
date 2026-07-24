// MCP server 状态与手动重启（设置页高级区面板）。
import { client } from "./client";

export interface McpServerStatus {
  name: string;
  status: string; // "running" | "down"
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
