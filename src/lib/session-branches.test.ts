import { describe, expect, it } from "vitest";
import type { SessionMeta } from "./chat";
import {
  buildSessionBranchRows,
  sessionBranchFamily,
  sessionBranchRootId,
  visibleSessionBranchRows,
} from "./session-branches";

const session = (id: string, patch: Partial<SessionMeta> = {}): SessionMeta => ({
  id,
  title: id,
  directory: "/repo",
  created_at: 1,
  updated_at: 1,
  ...patch,
});

describe("session branch projections", () => {
  it("按父子谱系输出稳定树，并让 root 限制保留整个可见分支族", () => {
    const sessions = [
      session("root-a", { updated_at: 5 }),
      session("child-a", { parent_id: "root-a", branch_root_id: "root-a", updated_at: 4 }),
      session("grandchild-a", {
        parent_id: "child-a",
        branch_root_id: "root-a",
        updated_at: 3,
      }),
      session("root-b", { updated_at: 2 }),
    ];
    const rows = buildSessionBranchRows(sessions);
    expect(rows.map((row) => [row.session.id, row.depth, row.descendantCount])).toEqual([
      ["root-a", 0, 2],
      ["child-a", 1, 1],
      ["grandchild-a", 2, 0],
      ["root-b", 0, 0],
    ]);
    expect(visibleSessionBranchRows(sessions, false, 1).map((row) => row.session.id)).toEqual([
      "root-a",
      "child-a",
      "grandchild-a",
    ]);
  });

  it("父分支删除后仍按稳定 root 归族并明确标记缺失父级", () => {
    const siblings = [
      session("child-a", { parent_id: "deleted", branch_root_id: "root-original" }),
      session("child-b", { parent_id: "deleted", branch_root_id: "root-original" }),
    ];
    expect(sessionBranchFamily(siblings[0]!, siblings).map((item) => item.id)).toEqual([
      "child-a",
      "child-b",
    ]);
    expect(buildSessionBranchRows(siblings).every((row) => row.parentMissing)).toBe(true);
  });

  it("存量 parent 链可推导根，循环谱系不会无限遍历或丢行", () => {
    const legacy = [session("root"), session("child", { parent_id: "root" })];
    expect(sessionBranchRootId(legacy[1]!, legacy)).toBe("root");
    const cycle = [session("a", { parent_id: "b" }), session("b", { parent_id: "a" })];
    expect(
      buildSessionBranchRows(cycle)
        .map((row) => row.session.id)
        .sort(),
    ).toEqual(["a", "b"]);
  });
});
