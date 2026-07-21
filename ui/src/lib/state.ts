// 会话状态：活跃会话 id + 会话列表（Sidebar 与 Session 页共享）。
import { createSignal } from "solid-js";
import { sessionCreate, sessionList, type SessionMeta } from "./chat";

export const [sessions, setSessions] = createSignal<SessionMeta[]>([]);
export const [activeSessionId, setActiveSessionId] = createSignal<string>("");

/** 启动时加载：无会话则创建一个，激活最新。 */
export async function initSessions(): Promise<void> {
  let list = await sessionList();
  if (list.length === 0) {
    const created = await sessionCreate();
    list = [created];
  }
  setSessions(list);
  if (!activeSessionId()) {
    setActiveSessionId(list[0].id);
  }
}

export async function refreshSessions(): Promise<void> {
  setSessions(await sessionList());
}

export async function newSession(): Promise<void> {
  const created = await sessionCreate();
  await refreshSessions();
  setActiveSessionId(created.id);
}

export function switchSession(id: string): void {
  setActiveSessionId(id);
}
