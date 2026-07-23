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

export async function currentModel(): Promise<{ provider: string; model: string }> {
  return client.rpc("current_model");
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
  running?: boolean;
}

export async function sessionUpdateMeta(
  id: string,
  patch: { title?: string; pinned?: boolean; sort_order?: number | null },
): Promise<void> {
  return client.rpc("session.update_meta", { id, ...patch });
}

export interface StoredPart {
  type: "text" | "context" | "tool_call" | "reasoning";
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

export async function sessionRewind(sessionId: string, messageId: string): Promise<void> {
  return client.rpc("session.rewind", { session_id: sessionId, message_id: messageId });
}

export async function sessionExport(sessionId: string): Promise<{ path: string }> {
  return client.rpc("session.export", { session_id: sessionId });
}

export interface WorktreeInfo {
  name: string;
  path: string;
  branch: string;
}

export async function worktreeList(): Promise<WorktreeInfo[]> {
  return client.rpc("worktree.list");
}

export async function worktreeCreate(name: string): Promise<WorktreeInfo> {
  return client.rpc("worktree.create", { name });
}

export async function worktreeRemove(name: string, deleteBranch = false): Promise<void> {
  return client.rpc("worktree.remove", { name, delete_branch: deleteBranch });
}

export async function worktreeStatus(path: string): Promise<{ path: string; status: string }[]> {
  return client
    .rpc<{ path: string; status: string }[]>("worktree.status", { path })
    .catch(() => []);
}

// ---------------- workspace ----------------

export interface Workspace {
  path: string;
  last_used: number;
}

export async function workspaceList(): Promise<Workspace[]> {
  return client.rpc<Workspace[]>("workspace.list");
}

export async function workspaceCurrent(): Promise<string> {
  return client.rpc<string>("workspace.current");
}

export async function workspaceAdd(path: string): Promise<void> {
  return client.rpc("workspace.add", { path });
}

export async function workspaceSwitch(path: string): Promise<void> {
  return client.rpc("workspace.switch", { path });
}

export interface WorkspaceOverview {
  path: string;
  sessions: number;
  running: number;
  last_activity: number;
  dirty: number | null;
}

export async function workspacesOverview(): Promise<WorkspaceOverview[]> {
  return client.rpc<WorkspaceOverview[]>("workspaces.overview");
}

// ---------------- diff（workdir 改动） ----------------

export interface DiffStatusEntry {
  path: string;
  status: string;
}

export async function diffStatus(): Promise<DiffStatusEntry[]> {
  return client.rpc<DiffStatusEntry[]>("diff.status");
}

export async function diffFile(path: string): Promise<string> {
  return client.rpc<string>("diff.file", { path });
}

// ---------------- agent 改动快照（本会话口径，与 git status 无关） ----------------

export interface AgentDiffEntry {
  path: string;
  added: number;
  deleted: number;
  status: "created" | "modified" | "deleted";
}

export async function agentDiffStatus(sessionId: string): Promise<AgentDiffEntry[]> {
  return client
    .rpc<AgentDiffEntry[]>("diff.agent_status", { session_id: sessionId })
    .catch(() => []);
}

export async function agentDiffFile(sessionId: string, path: string): Promise<string> {
  const r = await client
    .rpc<{ text: string }>("diff.agent_file", { session_id: sessionId, path })
    .catch(() => ({ text: "" }));
  return r.text;
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
  return client.rpc<GoalInfo[]>("goal.list");
}

export async function goalFocus(): Promise<GoalInfo | null> {
  return client.rpc<GoalInfo | null>("goal.focus");
}

export async function goalTransit(
  id: string,
  action: "activate" | "pause" | "resume" | "cancel",
): Promise<GoalInfo> {
  return client.rpc<GoalInfo>(`goal.${action}`, { id });
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
  return client.rpc<TaskInfo[]>("task.list");
}

export async function taskKill(id: string): Promise<boolean> {
  return client.rpc<boolean>("task.kill", { id });
}

// ---------------- 事件订阅（goal.update / task.update） ----------------

export function onTopic(
  topics: string[],
  handler: (topic: string, payload: unknown) => void,
): () => void {
  return client.stream(topics).on((payload) => handler("", payload));
}
