// 消息动作：fork / 重新生成 / 编辑重发（Session.tsx 拆出，350 门禁）。
import { sessionFork } from "./chat";
import { activeSessionId, refreshSessions, switchSession } from "./state";
import { flashErr } from "./flash";
import { formatError } from "./error-text";
import type { Item } from "./items";

type Send = (text: string, context: [], images: []) => Promise<void>;

/** 从指定消息分叉：新会话带前缀历史并切入。 */
export async function forkAt(messageId: string): Promise<void> {
  try {
    const forked = await sessionFork(activeSessionId(), messageId);
    await refreshSessions();
    switchSession(forked.id);
  } catch (e) {
    flashErr(`分叉失败：${formatError(e instanceof Error ? e.message : String(e))}`);
  }
}

/** 重新生成：把该 assistant 之前最近一条 user 消息重发一次。 */
export async function rerun(send: Send, items: Item[], idx: number): Promise<void> {
  for (let j = idx - 1; j >= 0; j--) {
    const m = items[j];
    if (m?.kind === "msg" && m.role === "user") {
      await send(m.content, [], []);
      return;
    }
  }
}

/** 编辑重发：fork 到该消息前一条（排除本消息），再发编辑后的文本。返回是否成功 fork。 */
export async function editResend(
  send: Send,
  items: Item[],
  idx: number,
  text: string,
): Promise<boolean> {
  for (let j = idx - 1; j >= 0; j--) {
    const m = items[j];
    if (m?.kind === "msg" && m.messageId) {
      try {
        const forked = await sessionFork(activeSessionId(), m.messageId);
        await refreshSessions();
        switchSession(forked.id);
        await send(text, [], []);
        return true;
      } catch (e) {
        // fork 失败不再继续往更早消息退避：那会静默丢失比用户预期更多的上下文
        flashErr(`编辑重发失败：${formatError(e instanceof Error ? e.message : String(e))}`);
        return false;
      }
    }
  }
  return false;
}
