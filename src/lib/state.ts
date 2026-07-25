// 会话状态：活跃会话 id + 会话列表（Sidebar 与 Session 页共享）。
import { createSignal } from "solid-js";
import { client } from "./client";
import { agentsList, type AgentActivity } from "./team";
import { sessionCreate, sessionDelete, sessionList, type SessionMeta } from "./chat";
import { applyDraftModel } from "./session-model";
import { migrateNewDraft } from "./drafts";

export const [sessions, setSessions] = createSignal<SessionMeta[]>([]);
export const [activeSessionId, setActiveSessionId] = createSignal<string>("");
/** 活跃会话是否已有对话内容（驱动右 dock 滑入/滑出）。 */
export const [hasConversation, setHasConversation] = createSignal(false);
/** 子代理名单（teammate/subagent/workflow 统一视图）。 */
export const [agents, setAgents] = createSignal<AgentActivity[]>([]);
/** PrimaryContent 选中项："" / "main" = 主会话，否则为 agent run 名（TopAgentBar chip 与右栏窗格共用）。 */
export const [activeAgentFocus, setActiveAgentFocus] = createSignal<string>("");

/** 当前选中是否为主会话。 */
export function isMainFocus(): boolean {
  const f = activeAgentFocus();
  return f === "" || f === "main";
}

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
  setActiveAgentFocus("");
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
  setActiveAgentFocus("");
  client.rpc("session.foreground", { id }).catch(() => {});
  navigate?.("/");
}

/** 删除会话并善后（SessionTree 行删除与 Cmd+W 共用）：错误上抛由调用方提示；
 *  删的是活跃会话则切同目录下一条，同目录无则切列表首条，全无回草稿态——activeSessionId 不得悬死。 */
export async function deleteSession(id: string): Promise<void> {
  const wasActive = activeSessionId() === id;
  const dir = sessions().find((s) => s.id === id)?.directory;
  await sessionDelete(id);
  await refreshSessions();
  if (!wasActive) return;
  const next = sessions().find((s) => s.directory === dir) ?? sessions()[0];
  if (next) switchSession(next.id);
  else await newSession();
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
