import { client } from "./client";
import type { McpServerStatus } from "./mcp";
import type { UsageCompleteness } from "./usage";
export interface DoctorEntry {
  provider: string;
  display: string;
  status: "imported" | "ok" | "missing" | "expired";
  detail: string;
}

export interface LspHealth {
  language: string;
  status: string;
}

/** 子系统健康汇总（MCP/LSP/MRM/event bus），仅 RPC 路径带（reprobe 纯凭证路径为 null）。 */
export interface SystemHealth {
  /** false 表示当前 Workspace 的 MCP runtime 尚未完成首次加载，此时空列表不是“未配置”。 */
  mcp_ready: boolean;
  mcp: McpServerStatus[];
  lsp_root: string;
  lsp: LspHealth[];
  mrm_describe: string;
  mrm_dispatches: number;
  bus_capacity: number;
  bus_receivers: number;
}

export interface DoctorReport {
  runtime: string;
  data_dir: string;
  config_dir: string;
  entries: DoctorEntry[];
  system?: SystemHealth | null;
}

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
  reasoning?: string;
  usage?: { input: number; output: number };
  error?: string;
}

/** 一次实际 Provider 路由的模型身份。消息上的值是生成时快照，不随当前配置变化。 */
export interface ModelIdentity {
  provider: string;
  model: string;
  account?: string | null;
}

export async function doctor(): Promise<DoctorReport> {
  return client.rpc<DoctorReport>("doctor");
}

export async function currentModel(sessionId?: string): Promise<ModelIdentity> {
  return client.rpc("current_model", sessionId ? { session_id: sessionId } : {});
}

export async function sendMessage(
  sessionId: string,
  text: string,
  context: ContextItem[] = [],
  images: Array<{ media_type: string; data: string }> = [],
): Promise<{ queued?: boolean }> {
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

export interface ApprovalRespondResult {
  resolved: boolean;
  rule_id?: string;
  rule_error?: string;
}

/** 审批应答；remember="session"|"workspace" 时放行成功后把命令全文建为前缀规则（B1）。 */
export async function approvalRespond(
  id: string,
  allow: boolean,
  remember?: "session" | "workspace",
): Promise<ApprovalRespondResult> {
  return client.rpc("approval.respond", remember ? { id, allow, remember } : { id, allow });
}

export interface PendingApproval {
  id: string;
  command: string;
  reason: string;
  session_id: string;
}

/** 等待中的审批（broker 300s 窗口内仍在等应答）。
 *  传 sessionId 恢复该 Session；省略时只恢复 Layout 全局审批，两者不会重复。 */
export async function approvalPending(sessionId?: string): Promise<PendingApproval[]> {
  return client.rpc<PendingApproval[]>(
    "approval.pending",
    sessionId ? { session_id: sessionId } : {},
  );
}

export async function sessionPendingList(sessionId: string): Promise<string[]> {
  return client.rpc<string[]>("session.pending_list", { id: sessionId });
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
  tokens: SessionUsage;
  ctx_pct: number;
  model: string;
}

/** 会话 token 累计；计量不完整时 input/output 是已知下限。 */
export interface SessionUsage extends UsageCompleteness {
  input: number;
  output: number;
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
  limits?: {
    daily_token_budget?: number | null;
    providers?: Record<
      string,
      {
        input_usd_per_million?: number | null;
        output_usd_per_million?: number | null;
        daily_cost_budget_usd?: number | null;
        circuit_failure_threshold?: number | null;
        circuit_cooldown_seconds?: number | null;
      }
    >;
  };
  experimental?: {
    automatic_knowledge_distillation?: boolean;
    browser_automation?: boolean;
    remote_mcp?: boolean;
  };
  composer_suggestions?: {
    enabled?: boolean;
    semantic?: boolean;
    llm?: boolean;
  };
  embedding?: {
    provider?: string;
    model?: string;
    base_url?: string;
  };
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
  parent_id?: string | null;
  branch_root_id?: string | null;
  fork_point?: {
    message_id: string;
    message_index: number;
    message_created_at: number;
    position: "before" | "after";
  } | null;
  fork_kind?: "manual" | "edit" | "rerun" | null;
  created_at: number;
  updated_at: number;
  pinned?: boolean;
  sort_order?: number | null;
  /** 会话级模型覆盖（缺省 = 跟随全局默认） */
  model?: ModelIdentity | null;
  running?: boolean;
}

export async function sessionUpdateMeta(
  id: string,
  patch: { title?: string; pinned?: boolean; sort_order?: number | null },
): Promise<void> {
  return client.rpc("session.update_meta", { id, ...patch });
}

export interface StoredPart {
  type: "text" | "context" | "context_sources" | "tool_call" | "reasoning" | "image" | "approval";
  text?: string;
  name?: string;
  input?: unknown;
  output?: string;
  args?: unknown;
  id?: string; // tool_call：provider 下发的 call id（仅供审计配对，前端不消费；存量 JSONL 缺省）
  media_type?: string;
  data?: string; // args=tool 精确 arguments；media_type/data=image 块
  // tool_call：执行起止 ms epoch（存量 JSONL 缺省 = unknown，不得虚构耗时）
  started_at?: number;
  finished_at?: number;
  command?: string; // approval 块：被审批的命令
  reason?: string; // approval 块：审批原因
  decision?: string; // approval 块：allow/deny/timeout/cancel
  items?: ContextItem[]; // context_sources：可逆的原始 typed @ 引用
}

/** 落盘的 run 终态统计（收尾 Assistant 消息携带；usage_complete=false 时 token 是已知下限）。 */
export interface MessageRunStats {
  ttft_ms: number;
  duration_ms: number;
  input_tokens: number;
  output_tokens: number;
  tokens_per_sec: number;
  usage_complete?: boolean;
}

export interface StoredMessage {
  id: string;
  session_id: string;
  role: "user" | "assistant" | "system";
  parts: StoredPart[];
  /** 生成本条 Assistant 的实际路由模型；旧 JSONL 和非 Assistant 消息缺省。 */
  model?: ModelIdentity | null;
  /** run 收尾消息的运行统计快照；旧 JSONL 缺省 = unknown。 */
  stats?: MessageRunStats | null;
  created_at: number;
}

export async function sessionList(): Promise<SessionMeta[]> {
  return client.rpc<SessionMeta[]>("session.list");
}

/** 会话运行态核对（resync 对账用）：RPC 失败/会话不在列表返回 null = 未知，调用方保守处理（不清 streaming 等下轮） */
export async function sessionRunning(id: string): Promise<boolean | null> {
  const list = await sessionList().catch(() => null);
  if (!list) return null;
  const s = list.find((x) => x.id === id);
  return s ? (s.running ?? false) : null;
}

export async function sessionCreate(directory?: string): Promise<SessionMeta> {
  return client.rpc<SessionMeta>("session.create", directory ? { directory } : {});
}

export async function sessionMessages(id: string): Promise<StoredMessage[]> {
  return client.rpc<StoredMessage[]>("session.messages", { id });
}

export async function sessionDelete(id: string, distill = false): Promise<void> {
  return client.rpc("session.delete", distill ? { id, distill: true } : { id });
}

export async function sessionFork(
  sessionId: string,
  messageId: string,
  options: {
    position: "before" | "after";
    kind: "manual" | "edit" | "rerun";
  } = { position: "after", kind: "manual" },
): Promise<SessionMeta> {
  return client.rpc<SessionMeta>("session.fork", {
    session_id: sessionId,
    message_id: messageId,
    ...options,
  });
}

export async function sessionRewind(
  sessionId: string,
  messageId: string,
  confirm = false,
): Promise<void> {
  return client.rpc("session.rewind", { session_id: sessionId, message_id: messageId, confirm });
}

/** 撤销最近一次回退：只恢复文件（对话在 rewind 时已截断，不回放）。 */
export async function sessionRewindUndo(sessionId: string): Promise<void> {
  return client.rpc("session.rewind_undo", { session_id: sessionId });
}

export async function sessionExport(sessionId: string): Promise<{ path: string }> {
  return client.rpc("session.export", { session_id: sessionId });
}

export * from "./chat-ops";
export * from "./composer-suggest";
