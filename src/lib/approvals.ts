// 审批事件处理：approval 事件入时间线 + 用户应答回写（Session.tsx 拆出，350 门禁）。
import { approvalRespond, type PendingApproval } from "./chat";
import type { ToolEvent } from "./delta";
import type { Item } from "./items";

type SetItems = (fn: (prev: Item[]) => Item[]) => void;

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
          },
        ],
  );
}

export async function respondApproval(
  setItems: SetItems,
  id: string,
  allow: boolean,
): Promise<void> {
  const r = await approvalRespond(id, allow).catch(() => null);
  // resolved:false = 服务端已了结（超时/取消/已被应答）的迟到应答：置失效，不冒充用户决定；
  // RPC 失败保持旧乐观行为（按用户点击上屏）
  const resolved =
    r?.resolved === false
      ? ("expired" as const)
      : allow
        ? ("allowed" as const)
        : ("denied" as const);
  setItems((prev) =>
    prev.map((it) => (it.kind === "approval" && it.approvalId === id ? { ...it, resolved } : it)),
  );
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
  }));
}
