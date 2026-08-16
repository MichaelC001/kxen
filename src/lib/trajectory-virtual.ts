// Trajectory 记录表虚拟化的纯逻辑：turn 树 -> 定高条目序列 -> 可视窗口。
// 条目高度是渲染约定（TrajectoryList 每个条目根节点用 style 强制同值），
// spacer/锚点/定位滚动全按这套常量换算，不做运行期测量——行高漂移在这里不存在。
import type { TrajectoryRecord, TrajectoryTurn } from "./trajectory";

export type VirtualEntry =
  | { key: string; type: "sep" }
  | { key: string; type: "turn-collapsed"; turn: TrajectoryTurn }
  | { key: string; type: "turn-head"; turn: TrajectoryTurn }
  | { key: string; type: "row"; record: TrajectoryRecord };

/** 定高表（px）：与 TrajectoryList 条目根节点的 style height 一一对应。 */
export const ENTRY_HEIGHT: Record<VirtualEntry["type"], number> = {
  sep: 9,
  "turn-collapsed": 24,
  "turn-head": 18,
  row: 24,
};

/** turn 树 -> 定高条目序列。sep 只在相邻 turn 之间；turn-head 只在 Collapse turns 开启且该 turn 已展开时出现。 */
export function flattenTurns(
  turns: TrajectoryTurn[],
  collapsed: (turn: TrajectoryTurn) => boolean,
  showHead: boolean,
): VirtualEntry[] {
  const out: VirtualEntry[] = [];
  turns.forEach((turn, i) => {
    if (i > 0) out.push({ key: `sep:${turn.startIndex}`, type: "sep" });
    if (collapsed(turn)) {
      out.push({ key: `tc:${turn.startIndex}`, type: "turn-collapsed", turn });
      return;
    }
    if (showHead) out.push({ key: `th:${turn.startIndex}`, type: "turn-head", turn });
    for (const record of turn.records) {
      out.push({ key: `row:${record.index}`, type: "row", record });
    }
  });
  return out;
}

export function entryHeight(entry: VirtualEntry): number {
  return ENTRY_HEIGHT[entry.type];
}

/** 条目下标 -> 顶部偏移（px）。 */
export function entryTop(entries: VirtualEntry[], index: number): number {
  let top = 0;
  const upto = Math.min(index, entries.length);
  for (let i = 0; i < upto; i++) top += entryHeight(entries[i]!);
  return top;
}

export function totalHeight(entries: VirtualEntry[]): number {
  return entryTop(entries, entries.length);
}

export interface VirtualWindow {
  /** 渲染区间 [start, end)（条目下标） */
  start: number;
  end: number;
  topPad: number;
  bottomPad: number;
}

/** 可视窗 + overscan 行缓冲 -> 渲染区间与上下 spacer 高度。视口为 0（未测量）时全量渲染兜底。 */
export function virtualWindow(
  entries: VirtualEntry[],
  scrollTop: number,
  viewport: number,
  overscanRows = 8,
): VirtualWindow {
  if (viewport <= 0) return { start: 0, end: entries.length, topPad: 0, bottomPad: 0 };
  let start = 0;
  while (
    start < entries.length &&
    entryTop(entries, start) + entryHeight(entries[start]!) <= scrollTop
  )
    start++;
  start = Math.max(0, start - overscanRows);
  let end = start;
  const limit = scrollTop + viewport;
  while (end < entries.length && entryTop(entries, end) < limit) end++;
  end = Math.min(entries.length, end + overscanRows);
  return {
    start,
    end,
    topPad: entryTop(entries, start),
    bottomPad: totalHeight(entries) - entryTop(entries, end),
  };
}

/** 视口首个（部分可见也算）条目的锚点：加载更早一页后据此恢复 scrollTop，视口内容原位不动。 */
export function topAnchor(
  entries: VirtualEntry[],
  scrollTop: number,
): { key: string; offset: number } | undefined {
  let index = 0;
  while (
    index < entries.length &&
    entryTop(entries, index) + entryHeight(entries[index]!) <= scrollTop
  )
    index++;
  const entry = entries[index];
  return entry ? { key: entry.key, offset: scrollTop - entryTop(entries, index) } : undefined;
}

/** 锚点 key -> 恢复后的 scrollTop；条目消失（搜索/折叠把锚点收走）回落 0。 */
export function anchorScrollTop(entries: VirtualEntry[], key: string, offset: number): number {
  const index = entries.findIndex((e) => e.key === key);
  return index < 0 ? 0 : entryTop(entries, index) + offset;
}

/** 记录 index -> 条目下标（无 = -1，Inspect 定位用）。 */
export function entryIndexOfRecord(entries: VirtualEntry[], recordIndex: number): number {
  return entries.findIndex((e) => e.type === "row" && e.record.index === recordIndex);
}
