// Done 对账（Cline 收敛副本）：run 结束以存储快照为最终权威，排队消息从后端队列取真源。
// stats/error 这类不进库的数据由调用方尾注重挂。
import {
  approvalPending,
  sessionMessages,
  sessionPendingClear,
  sessionPendingList,
  type RunStats,
} from "./chat";
import { pendingApprovalItems } from "./approvals";
import { toItems, type Item } from "./items";
import { activeSessionId } from "./state";

export function createConverge(deps: {
  setItems: (items: Item[]) => void;
  setPendingQueue: (q: string[]) => void;
  scroll: () => void;
}) {
  const converge = (
    sid: string,
    tail?: { stats?: RunStats | undefined; error?: string | undefined },
  ) => {
    void Promise.all([sessionMessages(sid), sessionPendingList(sid), approvalPending(sid)]).then(
      ([messages, q, pend]) => {
        if (activeSessionId() !== sid) return;
        const loaded = toItems(messages);
        const last = loaded.at(-1);
        if ((tail?.stats || tail?.error) && last?.kind === "msg" && last.role === "assistant") {
          loaded[loaded.length - 1] = { ...last, stats: tail?.stats, error: tail?.error };
        }
        deps.setItems([
          ...loaded,
          ...q.map((t) => ({ kind: "msg" as const, role: "user" as const, content: t })),
          // 对账是全量重建：仍在等的审批卡一并恢复，否则 Done 一刷等待卡凭空消失
          ...pendingApprovalItems(pend),
        ]);
        deps.setPendingQueue(q);
        deps.scroll();
      },
    );
  };

  const clearQueue = async () => {
    const sid = activeSessionId();
    if (!sid) return;
    await sessionPendingClear(sid);
    converge(sid); // 真源重载（乐观上屏的排队消息随快照撤下）
  };

  return { converge, clearQueue };
}
