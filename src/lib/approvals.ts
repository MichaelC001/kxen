// 审批事件处理：approval 事件入时间线 + 用户应答回写（Session.tsx 拆出，350 门禁）。
import { approvalRespond } from "./chat";
import type { ToolEvent } from "./delta";
import type { Item } from "./items";

type SetItems = (fn: (prev: Item[]) => Item[]) => void;

export function applyApprovalEvent(setItems: SetItems, event: ToolEvent): void {
  if (!event.approvalId) return;
  setItems((prev) => [
    ...prev,
    {
      kind: "approval",
      approvalId: event.approvalId!,
      command: event.command ?? "",
      reason: event.reason ?? "",
    },
  ]);
}

export async function respondApproval(
  setItems: SetItems,
  id: string,
  allow: boolean,
): Promise<void> {
  await approvalRespond(id, allow).catch(() => {});
  setItems((prev) =>
    prev.map((it) =>
      it.kind === "approval" && it.approvalId === id
        ? { ...it, resolved: (allow ? "allowed" : "denied") as "allowed" | "denied" }
        : it,
    ),
  );
}
