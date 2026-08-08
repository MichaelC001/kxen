import { client } from "./client";

// ---------------- kanban（看板 RPC，字段与后端 BoardState JSON 对齐） ----------------

/** 看板摘要（Workspaces 卡片徽标数据源，随 workspaces.overview 注入）。 */
export interface KanbanDigest {
  board_id: string;
  title: string;
  total_cards: number;
  waiting_human: number;
  running: number;
  blocked: number;
}

export interface KanbanBoardSummary extends KanbanDigest {
  policy: {
    allowlist: number;
    used: number;
    max_uses: number | null;
    expires_at_ms: number | null;
  } | null;
}

export interface KanbanColumn {
  id: string;
  title: string;
  on_enter: { kind: "none" | "agent_run" | "workflow" | "human_gate"; agent?: string | null };
  transitions: { on_success?: string | null; on_failure?: string | null };
  wip_limit?: number | null;
  timeout_ms?: number | null;
}

export type KanbanCardStatus = "ready" | "waiting_human" | "running" | "blocked";

export interface KanbanComment {
  author: string;
  body: string;
  at: number;
}

export interface KanbanCard {
  id: string;
  column_id: string;
  title: string;
  body: string;
  status: KanbanCardStatus;
  created_at: number;
  updated_at: number;
  current_run?: string | null;
  block_reason?: string | null;
  comments: KanbanComment[];
}

export interface KanbanRun {
  id: string;
  card_id: string;
  column_id: string;
  attempt: number;
  started_at: number;
  ended_at?: number | null;
  outcome?: "success" | "failure" | "timeout" | null;
}

export interface KanbanPolicySpec {
  allowlist: string[];
  expires_at_ms?: number | null;
  max_uses?: number | null;
}

/** 看板 agent 定义元数据（agent_defined 事件登记口径）；tools 仅 custom profile 有值。 */
export interface KanbanAgentDef {
  name: string;
  role: string;
  model: string;
  permission_profile: string;
  tools?: string[] | null;
  defined_at: number;
}

/** 完整板状态（kanban.snapshot，重连恢复口径）。 */
export interface KanbanSnapshot {
  board_id: string;
  title?: string | null;
  columns: KanbanColumn[];
  cards: Record<string, KanbanCard>;
  runs: Record<string, KanbanRun>;
  agents: Record<string, KanbanAgentDef>;
  policy?: { spec: KanbanPolicySpec; used: number } | null;
  seq: number;
}

interface KanbanLanded {
  event_id: string;
  seq: number;
}

export async function kanbanBoards(workspace: string): Promise<KanbanBoardSummary[]> {
  return client.rpc("kanban.boards", { workspace });
}

export async function kanbanSnapshot(workspace: string, board: string): Promise<KanbanSnapshot> {
  return client.rpc("kanban.snapshot", { workspace, board });
}

export async function kanbanBoardCreate(
  workspace: string,
  title: string,
): Promise<KanbanLanded & { board_id: string }> {
  return client.rpc("kanban.board_create", { workspace, title });
}

export async function kanbanCardCreate(
  workspace: string,
  board: string,
  title: string,
  body: string,
): Promise<KanbanLanded & { card_id: string }> {
  return client.rpc("kanban.card_create", { workspace, board, title, body });
}

export async function kanbanCardMove(
  workspace: string,
  board: string,
  cardId: string,
  outcome: "success" | "failure",
): Promise<KanbanLanded> {
  return client.rpc("kanban.card_move", { workspace, board, card_id: cardId, outcome });
}

export async function kanbanCardComment(
  workspace: string,
  board: string,
  cardId: string,
  body: string,
): Promise<KanbanLanded> {
  return client.rpc("kanban.card_comment", { workspace, board, card_id: cardId, body });
}

export async function kanbanRunStart(
  workspace: string,
  board: string,
  cardId: string,
): Promise<KanbanLanded> {
  return client.rpc("kanban.run_start", { workspace, board, card_id: cardId });
}

export async function kanbanPolicySet(
  workspace: string,
  board: string,
  policy: KanbanPolicySpec,
): Promise<KanbanLanded> {
  return client.rpc("kanban.policy_set", { workspace, board, policy });
}
