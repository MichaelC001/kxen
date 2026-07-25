// 会话组内排序逻辑（SessionTree 与测试共享）。
import type { SessionMeta } from "./chat";

/** 组内排序：置顶(updated 倒序) -> 手动序号(升序) -> 其余 updated 倒序。 */
export function sortGroup(list: SessionMeta[]): SessionMeta[] {
  const pinned = list.filter((s) => s.pinned).sort((a, b) => b.updated_at - a.updated_at);
  const ordered = list
    .filter((s) => !s.pinned && s.sort_order != null)
    .sort((a, b) => (a.sort_order ?? 0) - (b.sort_order ?? 0));
  const rest = list
    .filter((s) => !s.pinned && s.sort_order == null)
    .sort((a, b) => b.updated_at - a.updated_at);
  return [...pinned, ...ordered, ...rest];
}
