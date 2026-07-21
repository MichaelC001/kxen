import { rpc } from "./rpc";
import { subscribe } from "./stream";
import type { DoctorReport } from "./tauri";

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
  reasoning?: string;
  usage?: { input: number; output: number };
  error?: string;
}

export type { DoctorReport };

export async function doctor(): Promise<DoctorReport> {
  return rpc<DoctorReport>("doctor");
}

export async function currentModel(): Promise<{ provider: string; model: string }> {
  return rpc("current_model");
}

export async function setModel(provider: string, model: string): Promise<void> {
  return rpc("set_model", { provider, model });
}

export async function sendMessage(
  sessionId: string,
  text: string,
  context: ContextItem[] = [],
  images: Array<{ media_type: string; data: string }> = [],
): Promise<void> {
  return rpc("send_message", { session_id: sessionId, text, context, images });
}

export type ContextItem =
  | { type: "file"; path: string }
  | { type: "dir"; path: string }
  | { type: "web"; url: string }
  | { type: "docs"; url: string };

export interface CompleteEntry {
  path: string;
  kind: "file" | "dir";
}

export async function fsComplete(query: string, limit = 20): Promise<CompleteEntry[]> {
  return rpc<CompleteEntry[]>("fs.complete", { query, limit });
}

export interface CommandInfo {
  name: string;
  description: string;
  kind: "builtin" | "custom" | "skill";
  argument_hint?: string;
}

export async function commandList(): Promise<CommandInfo[]> {
  return rpc<CommandInfo[]>("command.list");
}

export async function sessionAbort(sessionId: string): Promise<boolean> {
  return rpc<boolean>("session.abort", { session_id: sessionId });
}

export interface StatuslineReport {
  items: string[];
  workdir: string;
  git_branch: string;
  goal?: { id: string; status: string } | null;
  tasks_running: number;
  tokens: { input: number; output: number };
  model: string;
}

export async function statusline(sessionId: string): Promise<StatuslineReport> {
  return rpc<StatuslineReport>("statusline", { session_id: sessionId });
}

export interface RoleBindingView {
  provider: string;
  model: string;
  fallback?: string | null;
}

export async function configGet(): Promise<{ roles: Record<string, RoleBindingView> }> {
  return rpc("config.get");
}

export async function configSetRole(
  role: string,
  provider: string,
  model: string,
  fallback?: string,
): Promise<void> {
  return rpc("config.set_role", { role, provider, model, fallback });
}

// ---------------- 会话 ----------------

export interface SessionMeta {
  id: string;
  title: string;
  directory: string;
  created_at: number;
  updated_at: number;
}

export interface StoredPart {
  type: "text" | "tool_call" | "reasoning";
  text?: string;
  name?: string;
  input?: unknown;
  output?: string;
}

export interface StoredMessage {
  id: string;
  session_id: string;
  role: "user" | "assistant" | "system";
  parts: StoredPart[];
  created_at: number;
}

export async function sessionList(): Promise<SessionMeta[]> {
  return rpc<SessionMeta[]>("session.list");
}

export async function sessionCreate(directory?: string): Promise<SessionMeta> {
  return rpc<SessionMeta>("session.create", directory ? { directory } : {});
}

export async function sessionMessages(id: string): Promise<StoredMessage[]> {
  return rpc<StoredMessage[]>("session.messages", { id });
}

export async function sessionDelete(id: string): Promise<void> {
  return rpc("session.delete", { id });
}

// ---------------- diff（workdir 改动） ----------------

export interface DiffStatusEntry {
  path: string;
  status: string;
}

export async function diffStatus(): Promise<DiffStatusEntry[]> {
  return rpc<DiffStatusEntry[]>("diff.status");
}

export async function diffFile(path: string): Promise<string> {
  return rpc<string>("diff.file", { path });
}

// ---------------- goal ----------------

export interface GoalInfo {
  id: string;
  status: string;
  objective: string;
  completion_criteria: string;
  constraints?: string | null;
  budget: { tokens?: number | null; turns?: number | null; wall_clock_ms?: number | null };
  turns_used: number;
  tokens_used: number;
  consecutive_blocks: number;
  block_reason?: string | null;
  verification_evidence?: string | null;
}

export async function goalList(): Promise<GoalInfo[]> {
  return rpc<GoalInfo[]>("goal.list");
}

export async function goalFocus(): Promise<GoalInfo | null> {
  return rpc<GoalInfo | null>("goal.focus");
}

export async function goalTransit(
  id: string,
  action: "activate" | "pause" | "resume" | "cancel",
): Promise<GoalInfo> {
  return rpc<GoalInfo>(`goal.${action}`, { id });
}

// ---------------- 后台任务 ----------------

export interface TaskInfo {
  id: string;
  command: string;
  status: "running" | "exited" | "killed" | "failed";
  uptime_ms: number;
  port?: number | null;
  tail: string;
}

export async function taskList(): Promise<TaskInfo[]> {
  return rpc<TaskInfo[]>("task.list");
}

export async function taskKill(id: string): Promise<boolean> {
  return rpc<boolean>("task.kill", { id });
}

// ---------------- 团队 ----------------

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
  status: "pending" | "in_progress" | "completed";
  assignee?: string | null;
  depends_on: number[];
}

export async function teamList(
  sessionId: string,
): Promise<{ members: TeamMember[]; tasks: TeamTask[] }> {
  return rpc("team.list", { session_id: sessionId });
}

export async function teamMessage(sessionId: string, name: string, text: string): Promise<void> {
  return rpc("team.message", { session_id: sessionId, name, text });
}

// ---------------- 事件订阅（goal.update / task.update） ----------------

export function onTopic(
  topics: string[],
  handler: (topic: string, payload: unknown) => void,
): Promise<() => void> {
  return subscribe(topics, handler);
}

export interface ToolEvent {
  kind: "tool_call" | "tool_result" | "phase";
  name: string;
  summary?: string;
}

export interface RunStats {
  ttft_ms: number;
  duration_ms: number;
  input_tokens: number;
  output_tokens: number;
  tokens_per_sec: number;
}

export function onLlmDelta(
  activeSession: () => string,
  onText: (text: string) => void,
  onReasoning: (text: string) => void,
  onDone: (stats?: RunStats, error?: string) => void,
  onTool?: (event: ToolEvent) => void,
): Promise<() => void> {
  return subscribe(["llm.delta"], (_topic, payload) => {
    handle(payload as DeltaPayload);
  });

  interface DeltaPayload {
    kind?: string;
    session_id?: string;
    text?: string;
    message?: string;
    name?: string;
    summary?: string;
    stats?: RunStats;
    agent?: string;
  }

  function handle(event: DeltaPayload) {
    // 只渲染活跃会话的增量（后台运行的其他会话事件忽略）
    if (event.session_id && event.session_id !== activeSession()) return;
    switch (event.kind) {
      case "text":
        if (event.text) onText(event.text);
        break;
      case "reasoning":
        if (event.text) onReasoning(event.text);
        break;
      case "done":
        onDone(event.stats);
        break;
      case "aborted":
        onDone(undefined, "(已中断)");
        break;
      case "error":
        onDone(undefined, event.message ?? "unknown error");
        break;
      case "tool_call":
      case "tool_result":
      case "phase":
        if (event.name) onTool?.({ kind: event.kind, name: event.name, summary: event.summary });
        break;
    }
  }
}
