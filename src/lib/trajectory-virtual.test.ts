// 虚拟化纯逻辑单测：扁平化（折叠/头部/分隔线）、窗口计算、锚点恢复、记录定位。
import { describe, expect, it } from "vitest";
import type { TrajectoryTurn } from "./trajectory";
import {
  anchorScrollTop,
  entryIndexOfRecord,
  entryTop,
  flattenTurns,
  topAnchor,
  totalHeight,
  virtualWindow,
  type VirtualEntry,
} from "./trajectory-virtual";

function turn(startIndex: number, recordCount: number): TrajectoryTurn {
  return {
    startIndex,
    records: Array.from({ length: recordCount }, (_, i) => ({
      index: startIndex + i,
      kind: "message" as const,
      messageId: `m${startIndex + i}`,
      partIndex: 0,
      time: 0,
      summary: `r${startIndex + i}`,
    })),
    steps: recordCount,
    toolCalls: 0,
    headed: true,
  };
}

const types = (entries: VirtualEntry[]) => entries.map((e) => e.type);

describe("flattenTurns", () => {
  const turns = [turn(0, 2), turn(2, 3)];

  it("展开态：turn 间插 sep，无头部行", () => {
    const entries = flattenTurns(turns, () => false, false);
    expect(types(entries)).toEqual(["row", "row", "sep", "row", "row", "row"]);
    expect(entries[2]?.key).toBe("sep:2");
  });

  it("Collapse turns：折叠 turn 只剩一行，展开 turn 多出头部计数行", () => {
    const collapsed = (t: TrajectoryTurn) => t.startIndex === 2;
    const entries = flattenTurns(turns, collapsed, true);
    expect(types(entries)).toEqual(["turn-head", "row", "row", "sep", "turn-collapsed"]);
  });
});

describe("virtualWindow 可视窗口", () => {
  // 10 条 row（24px 每条）= 240px 总高
  const entries = flattenTurns([turn(0, 10)], () => false, false);

  it("视口未测量（0）时全量渲染兜底", () => {
    expect(virtualWindow(entries, 0, 0)).toEqual({ start: 0, end: 10, topPad: 0, bottomPad: 0 });
  });

  it("中段滚动：只渲染可视窗 + overscan，上下 spacer 补齐总高", () => {
    const w = virtualWindow(entries, 96, 48, 1);
    // scrollTop 96 = 第 4 条起点；overscan 1 -> start 3；视口到 144 -> end 6 +1 = 7
    expect([w.start, w.end]).toEqual([3, 7]);
    expect(w.topPad + (w.end - w.start) * 24 + w.bottomPad).toBe(totalHeight(entries));
  });

  it("顶部与尾部边界不越界", () => {
    expect(virtualWindow(entries, 0, 240, 8)).toEqual({
      start: 0,
      end: 10,
      topPad: 0,
      bottomPad: 0,
    });
    const tail = virtualWindow(entries, 10_000, 48, 2);
    expect(tail.end).toBe(10);
    expect(tail.bottomPad).toBe(0);
  });
});

describe("锚点与定位", () => {
  const entries = flattenTurns([turn(0, 10)], () => false, false);

  it("topAnchor 取视口首个部分可见条目并保留条内位移", () => {
    expect(
      anchorScrollTop(entries, topAnchor(entries, 100)!.key, topAnchor(entries, 100)!.offset),
    ).toBe(100);
    expect(topAnchor([], 0)).toBeUndefined();
  });

  it("锚点 key 消失（搜索/折叠收走）时恢复回落 0", () => {
    expect(anchorScrollTop(entries.slice(0, 3), "row:9", 0)).toBe(0);
  });

  it("entryIndexOfRecord 按记录 index 定位", () => {
    expect(entryIndexOfRecord(entries, 4)).toBe(4);
    expect(entryIndexOfRecord(entries, 99)).toBe(-1);
    expect(entryTop(entries, 4)).toBe(96);
  });
});
