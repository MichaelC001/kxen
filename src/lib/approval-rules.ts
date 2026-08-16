// 审批规则与审批审计 RPC（B1/B7）：规则管理面 + 只读历史投影。
// approval.respond/approval.pending 留在 chat.ts（既有测试 mock 该模块）。
import { client } from "./client";

export interface ApprovalRule {
  id: string;
  prefix: string;
  scope: "session" | "workspace";
  session_id?: string;
  created_at_ms: number;
  expires_at_ms?: number;
  max_uses?: number;
  used: number;
  reason: string;
}

/** 审批规则列表：传 sessionId 含该会话规则 + 其 workspace 规则；省略时只看当前 workspace。 */
export async function approvalRulesList(sessionId?: string): Promise<ApprovalRule[]> {
  return client.rpc<ApprovalRule[]>(
    "approval_rules.list",
    sessionId ? { session_id: sessionId } : {},
  );
}

export async function approvalRulesRevoke(id: string, sessionId?: string): Promise<boolean> {
  const r = await client.rpc<{ revoked: boolean }>(
    "approval_rules.revoke",
    sessionId ? { id, session_id: sessionId } : { id },
  );
  return r.revoked;
}

export interface ApprovalHistoryEntry {
  session_id: string;
  created_at: number;
  command: string;
  reason: string;
  // allow / deny / timeout / cancel / rule_allow
  decision: string;
}

/** 审批审计（B7）：落盘 Part::Approval 投影，按时间倒序；省略 sessionId 为全局视图。 */
export async function approvalHistory(
  sessionId?: string,
  limit?: number,
): Promise<ApprovalHistoryEntry[]> {
  const params: Record<string, unknown> = {};
  if (sessionId) params.session_id = sessionId;
  if (limit !== undefined) params.limit = limit;
  return client.rpc<ApprovalHistoryEntry[]>("approval.history", params);
}
