import { client } from "./client";

// ---------------- 团队（teammate/subagent/workflow 统一注册） ----------------

export interface TeamMember {
  name: string;
  role: string;
  model: { provider: string; model: string };
  status: "working" | "idle" | "awaiting_plan_approval" | "failed" | "shutdown";
  plan_approval: boolean;
}

export interface TeamTask {
  id: number;
  title: string;
  status: "pending" | "in_progress" | "completed" | "failed" | "canceled";
  assignee?: string | null;
  depends_on: number[];
}

export async function teamList(
  sessionId: string,
): Promise<{ members: TeamMember[]; tasks: TeamTask[] }> {
  return client.rpc("team.list", { session_id: sessionId });
}

export async function teamMessage(sessionId: string, name: string, text: string): Promise<void> {
  return client.rpc("team.message", { session_id: sessionId, name, text });
}

export interface AgentActivity {
  name: string;
  kind: "teammate" | "subagent" | "workflow";
  model: { provider: string; model: string };
  status: "working" | "idle" | "awaiting_plan_approval" | "done" | "failed" | "shutdown";
  started_at: number;
}

export async function agentsList(sessionId: string): Promise<AgentActivity[]> {
  return client.rpc<AgentActivity[]>("agents.list", { session_id: sessionId });
}

export interface TranscriptEntry {
  kind?: string;
  text?: string;
  name?: string;
  summary?: string;
  message?: string;
}

export async function agentsTranscript(
  sessionId: string,
  name: string,
): Promise<TranscriptEntry[]> {
  return client.rpc<TranscriptEntry[]>("agents.transcript", { session_id: sessionId, name });
}

/** 按名停止 agent run：teammate 走 team shutdown，subagent/workflow 走取消句柄；不存在返回 false。 */
export async function agentsStop(sessionId: string, name: string): Promise<boolean> {
  return client.rpc<boolean>("agents.stop", { session_id: sessionId, name });
}
