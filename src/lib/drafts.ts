// 每会话草稿存储：模块级 Map 为运行时真源 + localStorage 写穿（重启不丢），key 为 session id。
// 未落库的新会话 activeSessionId 是 "" 且首发后才变真实 id，直接当键会清错键、旧键残留，
// 所以新会话统一用稳定键 DRAFT_NEW，落库后迁移到真实 id。

export const DRAFT_NEW = "draft:new";

const PREFIX = "kxen:draft:";
// localStorage 容量敏感：超 100KB 的草稿截断落盘并加可见标注（只限持久化副本，不打断在打的字）
const MAX_PERSIST = 100 * 1024;
const TRUNC_MARK = "\n[草稿过长，已截断]";

const drafts = new Map<string, string>();

// node 环境无 localStorage：退化为纯内存（懒取值，不锁死模块级引用）
function store(): Storage | null {
  return typeof localStorage === "undefined" ? null : localStorage;
}

export function draftKey(sessionId: string): string {
  return sessionId === "" ? DRAFT_NEW : sessionId;
}

export function getDraft(sessionId: string): string {
  const key = draftKey(sessionId);
  // Map 优先：写穿失败（配额满）时内存仍是真源；Map 未命中才回源 storage（冷启动恢复）
  const cached = drafts.get(key);
  if (cached !== undefined) return cached;
  const stored = store()?.getItem(PREFIX + key);
  if (stored != null) {
    drafts.set(key, stored);
    return stored;
  }
  return "";
}

export function setDraft(sessionId: string, text: string): void {
  const key = draftKey(sessionId);
  drafts.set(key, text);
  try {
    store()?.setItem(
      PREFIX + key,
      text.length > MAX_PERSIST ? text.slice(0, MAX_PERSIST) + TRUNC_MARK : text,
    );
  } catch {
    // 配额写满：内存 Map 仍是真源，放弃本次持久化（下次击键自然重试）
  }
}

/** 发送成功后清当前键：此刻真实 id 可能还没回来，新会话清的就是 DRAFT_NEW。 */
export function clearDraft(sessionId: string): void {
  const key = draftKey(sessionId);
  drafts.delete(key);
  store()?.removeItem(PREFIX + key);
}

function hasDraft(sessionId: string): boolean {
  const key = draftKey(sessionId);
  return drafts.has(key) || store()?.getItem(PREFIX + key) != null;
}

/** 新会话首发落库后调用：DRAFT_NEW 内容迁到真实 id 并清旧键（首发在途用户可能还在打字）。 */
export function migrateNewDraft(realSessionId: string): void {
  const pending = getDraft("");
  clearDraft("");
  if (pending && !hasDraft(realSessionId)) {
    setDraft(realSessionId, pending);
  }
}
