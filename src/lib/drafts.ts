// 每会话草稿存储：模块级 Map（组件重建不丢），key 为 session id。
// 未落库的新会话 activeSessionId 是 "" 且首发后才变真实 id，直接当键会清错键、旧键残留，
// 所以新会话统一用稳定键 DRAFT_NEW，落库后迁移到真实 id。

export const DRAFT_NEW = "draft:new";

const drafts = new Map<string, string>();

export function draftKey(sessionId: string): string {
  return sessionId === "" ? DRAFT_NEW : sessionId;
}

export function getDraft(sessionId: string): string {
  return drafts.get(draftKey(sessionId)) ?? "";
}

export function setDraft(sessionId: string, text: string): void {
  drafts.set(draftKey(sessionId), text);
}

/** 发送成功后清当前键：此刻真实 id 可能还没回来，新会话清的就是 DRAFT_NEW。 */
export function clearDraft(sessionId: string): void {
  drafts.delete(draftKey(sessionId));
}

/** 新会话首发落库后调用：DRAFT_NEW 内容迁到真实 id 并清旧键（首发在途用户可能还在打字）。 */
export function migrateNewDraft(realSessionId: string): void {
  const pending = drafts.get(DRAFT_NEW);
  drafts.delete(DRAFT_NEW);
  if (pending && !drafts.has(realSessionId)) {
    drafts.set(realSessionId, pending);
  }
}
