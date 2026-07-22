// 会话排序逻辑实测（A2）：sortGroup 三段序 + moveItem 拖拽换位。
import { describe, expect, it } from "vitest";
import { moveItem, sortGroup } from "../lib/order";
import type { SessionMeta } from "../lib/chat";

function ses(id: string, opt: Partial<SessionMeta> = {}): SessionMeta {
  return {
    id,
    title: id,
    directory: "/d",
    created_at: 0,
    updated_at: 0,
    ...opt,
  };
}

describe("session ordering", () => {
  it("置顶优先，手动序号次之，其余按更新时间倒序", () => {
    const list = [
      ses("rest-new", { updated_at: 300 }),
      ses("ordered-1", { sort_order: 1, updated_at: 100 }),
      ses("pinned-old", { pinned: true, updated_at: 50 }),
      ses("rest-old", { updated_at: 100 }),
      ses("ordered-2", { sort_order: 2, updated_at: 200 }),
      ses("pinned-new", { pinned: true, updated_at: 400 }),
    ];
    expect(sortGroup(list).map((s) => s.id)).toEqual([
      "pinned-new",
      "pinned-old",
      "ordered-1",
      "ordered-2",
      "rest-new",
      "rest-old",
    ]);
  });

  it("moveItem 前移与后移", () => {
    expect(moveItem(["a", "b", "c", "d"], 0, 2)).toEqual(["b", "c", "a", "d"]);
    expect(moveItem(["a", "b", "c", "d"], 3, 1)).toEqual(["a", "d", "b", "c"]);
    expect(moveItem(["a", "b"], -1, 5)).toEqual(["a", "b"]);
  });
});
