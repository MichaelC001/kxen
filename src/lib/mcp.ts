// MCP server 状态与手动重启（设置页高级区面板）。
import { client } from "./client";

export interface McpServerStatus {
  name: string;
  status: string; // "running" | "down"
  tools: number;
}

export function mcpStatus(): Promise<McpServerStatus[]> {
  return client.rpc("mcp.status");
}

export function mcpRestart(name: string): Promise<void> {
  return client.rpc("mcp.restart", { name });
}
