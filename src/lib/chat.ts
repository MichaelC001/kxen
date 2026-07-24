import { client } from "./client";
export interface DoctorEntry {
  provider: string;
  display: string;
  status: "imported" | "ok" | "missing" | "expired";
  detail: string;
}

export interface DoctorReport {
  entries: DoctorEntry[];
  bun_like_runtime: string;
  data_dir: string;
  config_dir: string;
}

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
  reasoning?: string;
  usage?: { input: number; output: number };
  error?: string;
}

export async function doctor(): Promise<DoctorReport> {
  return client.rpc<DoctorReport>("doctor");
}

export async function currentModel(
  sessionId?: string,
): Promise<{ provider: string; model: string }> {
  return client.rpc("current_model", sessionId ? { session_id: sessionId } : {});
}

export async function setModel(provider: string, model: string): Promise<void> {
  return client.rpc("set_model", { provider, model });
}

export async function sendMessage(
  sessionId: string,
  text: string,
  context: ContextItem[] = [],
  images: Array<{ media_type: string; data: string }> = [],
): Promise<{ queued?: boolean; stream_id?: string }> {
  return client.rpc("send_message", { session_id: sessionId, text, context, images });
}

export type ContextItem =
  | { type: "file"; path: string }
  | { type: "dir"; path: string }
  | { type: "web"; url: string }
  | { type: "docs"; url: string }
  | { type: "note"; text: string };

export interface CompleteEntry {
  path: string;
  kind: "file" | "dir";
}

export async function fsComplete(query: string, limit = 20): Promise<CompleteEntry[]> {
  return client.rpc<CompleteEntry[]>("fs.complete", { query, limit });
}

export interface CommandInfo {
  name: string;
  description: string;
  kind: "builtin" | "custom" | "skill";
  argument_hint?: string;
}

export async function commandList(): Promise<CommandInfo[]> {
  return client.rpc<CommandInfo[]>("command.list");
}

export async function sessionAbort(sessionId: string): Promise<boolean> {
  return client.rpc<boolean>("session.abort", { session_id: sessionId });
}

export { onLlmDelta } from "./delta";
export type { RunStats, ToolEvent } from "./delta";

export async function approvalRespond(id: string, allow: boolean): Promise<void> {
  return client.rpc("approval.respond", { id, allow });
}

export async function sessionPendingList(sessionId: string): Promise<string[]> {
  return client.rpc<string[]>("session.pending_list", { id: sessionId }).catch(() => []);
}

export async function sessionPendingClear(sessionId: string): Promise<void> {
  return client.rpc("session.pending_clear", { id: sessionId });
}

export interface StatuslineReport {
  items: string[];
  workdir: string;
  git_branch: string;
  goal?: { id: string; status: string } | null;
  tasks_running: number;
  tokens: { input: number; output: number };
  ctx_pct: number;
  model: string;
}

export async function statusline(sessionId: string): Promise<StatuslineReport> {
  return client.rpc<StatuslineReport>("statusline", { session_id: sessionId });
}

export interface RoleBindingView {
  provider: string;
  model: string;
  fallback?: string | null;
  account?: string | null;
}

export async function configGet(): Promise<{
  roles: Record<string, RoleBindingView>;
  send_when_running?: string;
}> {
  return client.rpc("config.get");
}

export async function configSetRole(
  role: string,
  provider: string,
  model: string,
  fallback?: string,
  account?: string,
): Promise<void> {
  return client.rpc("config.set_role", { role, provider, model, fallback, account });
}

// ---------------- 会话 ----------------

export interface SessionMeta {
  id: string;
  title: string;
  directory: string;
  created_at: number;
  updated_at: number;
  pinned?: boolean;
  sort_order?: number | null;
  /** 会话级模型覆盖（缺省 = 跟随全局默认） */
  model?: { provider: string; model: string; account?: string | null } | null;
  running?: boolean;
}

export async function sessionUpdateMeta(
  id: string,
  patch: { title?: string; pinned?: boolean; sort_order?: number | null },
): Promise<void> {
  return client.rpc("session.update_meta", { id, ...patch });
}

export interface StoredPart {
  type: "text" | "context" | "tool_call" | "reasoning" | "image";
  text?: string;
  name?: string;
  input?: unknown;
  output?: string;
  args?: unknown;
  media_type?: string;
  data?: string; // args=tool 精确 arguments；media_type/data=image 块
}

export interface StoredMessage {
  id: string;
  session_id: string;
  role: "user" | "assistant" | "system";
  parts: StoredPart[];
  created_at: number;
}

export async function sessionList(): Promise<SessionMeta[]> {
  return client.rpc<SessionMeta[]>("session.list");
}

export async function sessionCreate(directory?: string): Promise<SessionMeta> {
  return client.rpc<SessionMeta>("session.create", directory ? { directory } : {});
}

export async function sessionMessages(id: string): Promise<StoredMessage[]> {
  return client.rpc<StoredMessage[]>("session.messages", { id });
}

export async function sessionDelete(id: string): Promise<void> {
  return client.rpc("session.delete", { id });
}

export async function sessionFork(sessionId: string, messageId: string): Promise<SessionMeta> {
  return client.rpc<SessionMeta>("session.fork", { session_id: sessionId, message_id: messageId });
}

export async function sessionRewind(
  sessionId: string,
  messageId: string,
  confirm = false,
): Promise<void> {
  return client.rpc("session.rewind", { session_id: sessionId, message_id: messageId, confirm });
}

export async function sessionExport(sessionId: string): Promise<{ path: string }> {
  return client.rpc("session.export", { session_id: sessionId });
}

export * from "./chat-ops";
