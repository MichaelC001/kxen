// session.context_stats RPC：composer 上下文占用指示的数据源。
// 三段拆分是 chars/4 估算（后端 ws/context_stats.rs 口径），展示一律带 ~；
// last_input_tokens 是最近一次 run 的 provider 实测输入（精确锚点，null = 尚无实测）。
import { client } from "./client";

export interface ContextStats {
  system_tokens: number;
  tool_tokens: number;
  message_tokens: number;
  window_tokens: number;
  last_input_tokens?: number | null;
}

export async function sessionContextStats(sessionId: string): Promise<ContextStats> {
  return client.rpc<ContextStats>("session.context_stats", { session_id: sessionId });
}
