// 审批事件处理：approval 事件入时间线 + 用户应答回写。
import { approvalRespond, type PendingApproval } from "./chat";
import { flashErr } from "./flash";
import { formatError } from "./error-text";
import type { ToolEvent } from "./delta";
import type { Item } from "./items";

type SetItems = (fn: (prev: Item[]) => Item[]) => void;
const responding = new Set<string>();

export function applyApprovalEvent(setItems: SetItems, event: ToolEvent): void {
  if (!event.approvalId) return;
  setItems((prev) =>
    // 重载恢复（approval.pending）与实时事件可能撞车：同 id 只留一张卡
    prev.some((it) => it.kind === "approval" && it.approvalId === event.approvalId)
      ? prev
      : [
          ...prev,
          {
            kind: "approval",
            approvalId: event.approvalId!,
            command: event.command ?? "",
            reason: event.reason ?? "",
            sessionId: event.sessionId,
          },
        ],
  );
}

export async function respondApproval(
  setItems: SetItems,
  id: string,
  allow: boolean,
  remember?: "session" | "workspace",
): Promise<boolean> {
  if (responding.has(id)) return false;
  responding.add(id);
  // RPC 失败不上假已决态：后端 broker 仍在等应答，保持等待卡（用户可重试或等超时事件），
  // 错误上屏让用户感知失败——假已决态会让用户以为已应答，实际命令仍挂起
  try {
    // 不带 remember 时不传第三参（既有调用方与测试断言保持两参形态）
    const r = await (
      remember ? approvalRespond(id, allow, remember) : approvalRespond(id, allow)
    ).catch((e: unknown) => {
      flashErr(`审批应答失败：${formatError(e)}`);
      return null;
    });
    if (r === null) return false;
    // 建规失败不撤回放行（后端语义）：用户决定已生效，但规则没落表，必须明示
    if (r.rule_error) flashErr(`命令已放行，但审批规则未保存：${r.rule_error}`);
    // resolved:false = 服务端已了结（超时/取消/已被应答）的迟到应答：置失效，不冒充用户决定
    const resolved =
      r.resolved === false
        ? ("expired" as const)
        : allow
          ? ("allowed" as const)
          : ("denied" as const);
    setItems((prev) =>
      prev.map((it) => (it.kind === "approval" && it.approvalId === id ? { ...it, resolved } : it)),
    );
    return true;
  } finally {
    responding.delete(id);
  }
}

/** 后端了结事件（approval.resolved）：只标记还没应答的卡——用户已决定的不许被迟到事件改写。 */
export function applyApprovalResolved(setItems: SetItems, id: string, outcome: string): void {
  const resolved = outcome === "timeout" ? "timeout" : "cancelled";
  setItems((prev) =>
    prev.map((it) =>
      it.kind === "approval" && it.approvalId === id && !it.resolved ? { ...it, resolved } : it,
    ),
  );
}

/** approval.pending 快照 -> 等待中审批卡（会话重载恢复）。
 *  已决的审批由 toItems 从落盘 Part 渲染，pending 只含未决——两者互补不重叠。 */
export function pendingApprovalItems(list: PendingApproval[]): Item[] {
  return list.map((a) => ({
    kind: "approval",
    approvalId: a.id,
    command: a.command,
    reason: a.reason,
    sessionId: a.session_id || undefined,
  }));
}
