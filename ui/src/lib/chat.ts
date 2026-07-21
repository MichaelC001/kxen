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

export async function sendMessage(sessionId: string, text: string): Promise<void> {
  return rpc("send_message", { session_id: sessionId, text });
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

export function onLlmDelta(
  activeSession: () => string,
  onText: (text: string) => void,
  onReasoning: (text: string) => void,
  onDone: (usage?: { input: number; output: number }, error?: string) => void,
  onTool?: (event: ToolEvent) => void,
): Promise<() => void> {
  let usage: { input: number; output: number } | undefined;
  return subscribe(["llm.delta"], (_topic, payload) => {
    handle(
      payload as {
        kind?: string;
        session_id?: string;
        text?: string;
        input?: number;
        output?: number;
        message?: string;
        name?: string;
        summary?: string;
      },
    );
  });

  function handle(event: {
    kind?: string;
    session_id?: string;
    text?: string;
    input?: number;
    output?: number;
    message?: string;
    name?: string;
    summary?: string;
  }) {
    // 只渲染活跃会话的增量（后台运行的其他会话事件忽略）
    if (event.session_id && event.session_id !== activeSession()) return;
    switch (event.kind) {
      case "text":
        if (event.text) onText(event.text);
        break;
      case "reasoning":
        if (event.text) onReasoning(event.text);
        break;
      case "usage":
        usage = { input: event.input ?? 0, output: event.output ?? 0 };
        break;
      case "done":
        onDone(usage);
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
