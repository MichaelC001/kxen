// 消息动作：fork / 重新生成 / 编辑重发。
import { sessionFork, type ContextItem } from "./chat";
import {
  activeSessionId,
  captureSessionIntent,
  isSessionIntentCurrent,
  refreshSessions,
  switchSession,
} from "./state";
import { flashErr, flashOk } from "./flash";
import { formatError } from "./error-text";
import type { Item } from "./items";
import type { SendResult } from "./send";

type Send = (
  text: string,
  context: ContextItem[],
  images: Array<{ media_type: string; data: string }>,
) => Promise<SendResult>;

type RestoreFailedSend = (
  sessionId: string,
  text: string,
  context: ContextItem[],
  images: Array<{ media_type: string; data: string }>,
) => void;

export async function switchBranch(id: string): Promise<void> {
  if (id === activeSessionId()) return;
  try {
    await switchSession(id);
  } catch (error) {
    flashErr(`切换分支失败：${formatError(error)}`);
  }
}

async function activateFork(
  id: string,
  action: string,
  originSessionId: string,
  originIntent: number,
): Promise<boolean> {
  let refreshError: unknown;
  try {
    await refreshSessions();
  } catch (error) {
    refreshError = error;
  }
  if (!isSessionIntentCurrent(originIntent, originSessionId)) {
    flashErr(`${action}已创建（${id}），但当前会话已切换，未自动切入`);
    return false;
  }
  try {
    await switchSession(id);
  } catch (error) {
    flashErr(
      `${action}已创建（${id}），但切换失败：${formatError(error)}${
        refreshError ? `；列表刷新也失败：${formatError(refreshError)}` : ""
      }`,
    );
    return false;
  }
  if (activeSessionId() !== id) {
    flashErr(`${action}已创建（${id}），但当前会话已切换，未自动切入`);
    return false;
  }
  if (refreshError) {
    flashErr(`${action}已创建并切入，但会话列表刷新失败：${formatError(refreshError)}`);
  }
  return true;
}

const forkFlights = new Map<string, Promise<void>>();

/** 从指定消息分叉：同一会话同一消息的连点共享一次创建。 */
export function forkAt(messageId: string): Promise<void> {
  const originSessionId = activeSessionId();
  const key = `${originSessionId}\u0000${messageId}`;
  const current = forkFlights.get(key);
  if (current) return current;
  const originIntent = captureSessionIntent();
  const flight = performFork(messageId, originSessionId, originIntent).finally(() => {
    if (forkFlights.get(key) === flight) forkFlights.delete(key);
  });
  forkFlights.set(key, flight);
  return flight;
}

async function performFork(
  messageId: string,
  originSessionId: string,
  originIntent: number,
): Promise<void> {
  let forked: Awaited<ReturnType<typeof sessionFork>>;
  try {
    forked = await sessionFork(originSessionId, messageId, {
      position: "after",
      kind: "manual",
    });
  } catch (e) {
    flashErr(`分叉失败：${formatError(e)}`);
    return;
  }
  await activateFork(forked.id, "分叉", originSessionId, originIntent);
}

/** 重新生成：从关联 user 消息之前创建独立分支，再原样发送该消息。原回复永久保留。 */
const rerunFlights = new Map<string, Promise<void>>();

export function rerun(
  send: Send,
  items: Item[],
  idx: number,
  restoreFailedSend: RestoreFailedSend = () => {},
): Promise<void> {
  const target = items[idx];
  const targetKey = target?.kind === "msg" ? (target.messageId ?? String(idx)) : String(idx);
  const key = `${activeSessionId()}\u0000${targetKey}`;
  const current = rerunFlights.get(key);
  if (current) return current;
  const flight = performRerun(send, items, idx, restoreFailedSend).finally(() => {
    if (rerunFlights.get(key) === flight) rerunFlights.delete(key);
  });
  rerunFlights.set(key, flight);
  return flight;
}

async function performRerun(
  send: Send,
  items: Item[],
  idx: number,
  restoreFailedSend: RestoreFailedSend,
): Promise<void> {
  for (let j = idx - 1; j >= 0; j--) {
    const m = items[j];
    if (m?.kind === "msg" && m.role === "user") {
      if (m.contextUnavailable) {
        flashErr("旧消息的 @ 引用不可恢复，无法安全重新生成；请手动重新选择引用");
        return;
      }
      if (!m.messageId) {
        flashErr("重新生成失败：原用户消息尚未持久化");
        return;
      }
      const originSessionId = activeSessionId();
      const originIntent = captureSessionIntent();
      let forked: Awaited<ReturnType<typeof sessionFork>>;
      try {
        forked = await sessionFork(originSessionId, m.messageId, {
          position: "before",
          kind: "rerun",
        });
      } catch (e) {
        flashErr(`重新生成失败：${formatError(e)}`);
        return;
      }
      const context = m.context ?? [];
      const images = m.images ?? [];
      if (!(await activateFork(forked.id, "重新生成分支", originSessionId, originIntent))) {
        restoreFailedSend(forked.id, m.content, context, images);
        return;
      }
      try {
        const result = await send(m.content, context, images);
        if (!result.admitted) restoreFailedSend(forked.id, m.content, context, images);
        if (result.queued) flashOk("已加入队列，当前回复完成后自动发送");
      } catch (e) {
        restoreFailedSend(forked.id, m.content, context, images);
        flashErr(`重新生成失败：${formatError(e)}`);
      }
      return;
    }
  }
}

/** 编辑重发：从该消息之前创建独立分支，再发送编辑文本；首条消息同样保留完整谱系与模型设置。 */
export async function editResend(
  send: Send,
  items: Item[],
  idx: number,
  text: string,
  restoreFailedSend: RestoreFailedSend = () => {},
): Promise<boolean> {
  const target = items[idx];
  if (target?.kind === "msg" && target.contextUnavailable) {
    flashErr("旧消息的 @ 引用不可恢复，无法安全编辑重发；请复制文本并重新选择引用");
    return false;
  }
  const images = target?.kind === "msg" ? (target.images ?? []) : [];
  const context = target?.kind === "msg" ? (target.context ?? []) : [];
  if (target?.kind !== "msg" || !target.messageId) {
    flashErr("编辑重发失败：原消息尚未持久化");
    return false;
  }
  const originSessionId = activeSessionId();
  const originIntent = captureSessionIntent();
  let forked: Awaited<ReturnType<typeof sessionFork>>;
  try {
    forked = await sessionFork(originSessionId, target.messageId, {
      position: "before",
      kind: "edit",
    });
  } catch (e) {
    if (!isSessionIntentCurrent(originIntent, originSessionId)) {
      restoreFailedSend(originSessionId, text, context, images);
    }
    flashErr(`编辑重发失败：${formatError(e)}`);
    return false;
  }
  if (!(await activateFork(forked.id, "编辑分支", originSessionId, originIntent))) {
    restoreFailedSend(forked.id, text, context, images);
    return false;
  }
  try {
    const result = await send(text, context, images);
    if (!result.admitted) restoreFailedSend(forked.id, text, context, images);
    return result.admitted;
  } catch (e) {
    restoreFailedSend(forked.id, text, context, images);
    flashErr(`编辑重发失败：${formatError(e)}`);
    return false;
  }
}
