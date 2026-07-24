import { client } from "./client";

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
