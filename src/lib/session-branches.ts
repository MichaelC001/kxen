import type { SessionMeta } from "./chat";
import { sortGroup } from "./order";

export interface SessionBranchRow {
  session: SessionMeta;
  depth: number;
  rootId: string;
  parentMissing: boolean;
  descendantCount: number;
}

function byDirectory(sessions: SessionMeta[]): Map<string, SessionMeta> {
  return new Map(sessions.map((session) => [session.id, session]));
}

/** 新格式优先读取稳定 root；存量 Session 沿 parent 链解析并对环、缺失父级 fail safe。 */
export function sessionBranchRootId(session: SessionMeta, sessions: SessionMeta[]): string {
  if (session.branch_root_id) return session.branch_root_id;
  const byId = byDirectory(
    sessions.filter((candidate) => candidate.directory === session.directory),
  );
  let current = session;
  const seen = new Set([current.id]);
  while (current.parent_id) {
    if (seen.has(current.parent_id)) return session.id;
    const parent = byId.get(current.parent_id);
    if (!parent) return current.id;
    seen.add(parent.id);
    current = parent;
  }
  return current.id;
}

export function sessionBranchFamily(session: SessionMeta, sessions: SessionMeta[]): SessionMeta[] {
  const inDirectory = sessions.filter((candidate) => candidate.directory === session.directory);
  const rootId = sessionBranchRootId(session, inDirectory);
  return inDirectory.filter((candidate) => sessionBranchRootId(candidate, inDirectory) === rootId);
}

/** 把扁平 Session catalog 投影为稳定父子树。缺失父级和环不会让 Session 从列表消失。 */
export function buildSessionBranchRows(sessions: SessionMeta[]): SessionBranchRow[] {
  const byId = byDirectory(sessions);
  const children = new Map<string, SessionMeta[]>();
  const roots: SessionMeta[] = [];
  for (const session of sessions) {
    const parent = session.parent_id ? byId.get(session.parent_id) : undefined;
    if (!parent || parent.directory !== session.directory) roots.push(session);
    else children.set(parent.id, [...(children.get(parent.id) ?? []), session]);
  }
  const rows: SessionBranchRow[] = [];
  const visited = new Set<string>();
  const append = (session: SessionMeta, depth: number, rootId: string, parentMissing: boolean) => {
    if (visited.has(session.id)) return;
    visited.add(session.id);
    const start = rows.length;
    rows.push({ session, depth, rootId, parentMissing, descendantCount: 0 });
    for (const child of sortGroup(children.get(session.id) ?? [])) {
      append(child, depth + 1, rootId, false);
    }
    rows[start]!.descendantCount = rows.length - start - 1;
  };
  for (const root of sortGroup(roots)) {
    append(root, 0, root.id, Boolean(root.parent_id));
  }
  // 只有损坏或循环谱系才会剩余；仍作为可见根展示，不静默丢 Session。
  for (const orphan of sortGroup(sessions.filter((session) => !visited.has(session.id)))) {
    append(orphan, 0, orphan.id, true);
  }
  return rows;
}

export function visibleSessionBranchRows(
  sessions: SessionMeta[],
  expanded: boolean,
  maxRoots: number,
): SessionBranchRow[] {
  const rows = buildSessionBranchRows(sessions);
  if (expanded) return rows;
  const roots = rows.filter((row) => row.depth === 0).slice(0, maxRoots);
  const visibleRoots = new Set(roots.map((row) => row.rootId));
  return rows.filter((row) => visibleRoots.has(row.rootId));
}

export function forkKindLabel(kind: SessionMeta["fork_kind"]): string {
  if (kind === "edit") return "编辑分支";
  if (kind === "rerun") return "重新生成分支";
  return "手动分支";
}
