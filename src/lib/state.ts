// 会话状态：活跃会话 id + 会话列表（Sidebar 与 Session 页共享）。
import { createSignal } from "solid-js";
import { client } from "./client";
import { agentsList, type AgentActivity } from "./team";
import { sessionCreate, sessionList, type SessionMeta } from "./chat";
import { applyDraftModel } from "./session-model";
import { migrateNewDraft } from "./drafts";

export const [sessions, setSessions] = createSignal<SessionMeta[]>([]);
export const [activeSessionId, setActiveSessionId] = createSignal<string>("");
/** 活跃会话是否已有对话内容（驱动右 dock 滑入/滑出）。 */
export const [hasConversation, setHasConversation] = createSignal(false);
/** 子代理名单（teammate/subagent/workflow 统一视图）。 */
export const [agents, setAgents] = createSignal<AgentActivity[]>([]);
/** 当前 focus 的子代理名（null = 显示主会话上下文）。 */
export const [focusAgent, setFocusAgent] = createSignal<string | null>(null);

/** 启动时加载：无会话则创建一个，激活最新。 */
export async function initSessions(): Promise<void> {
  let list = await sessionList();
  if (list.length === 0) {
    const created = await sessionCreate();
    list = [created];
  }
  setSessions(list);
  if (!activeSessionId() && list[0]) {
    setActiveSessionId(list[0].id);
  }
}

export async function refreshSessions(): Promise<void> {
  setSessions(await sessionList());
}

/** 路由导航 hook（App 装配时注入；state 不直接依赖 router）。 */
let nav: ((path: string) => void) | null = null;
export function setNavigator(fn: (path: string) => void): void {
  nav = fn;
}

/** 已注入则跳转，未注入静默（测试环境）。 */
export function navigate(path: string): void {
  nav?.(path);
}

export async function newSession(): Promise<void> {
  // 草稿态：不立即落库；首次发送消息时才创建会话（对齐 Cursor/Claude/ChatGPT）
  setActiveSessionId("");
  setFocusAgent(null);
  navigate?.("/");
}

/** 草稿态首条消息：先落库成会话再激活。返回活跃会话 id。 */
export async function ensureActiveSession(): Promise<string> {
  const existing = activeSessionId();
  if (existing) return existing;
  const created = await sessionCreate();
  await applyDraftModel(created.id);
  await refreshSessions();
  // 先迁移草稿键再激活：激活触发的 composer 恢复要读到迁移后的内容
  migrateNewDraft(created.id);
  setActiveSessionId(created.id);
  client.rpc("session.foreground", { id: created.id }).catch(() => {});
  return created.id;
}

export function switchSession(id: string): void {
  setActiveSessionId(id);
  setFocusAgent(null);
  client.rpc("session.foreground", { id }).catch(() => {});
  navigate?.("/");
}

/** 刷新子代理名单（3s 轮询 + 事件驱动调用方）。 */
export async function refreshAgents(): Promise<void> {
  const sid = activeSessionId();
  if (!sid) {
    setAgents([]);
    return;
  }
  setAgents(await agentsList(sid).catch(() => []));
}
