// 看板纯逻辑实测：rankCards 活跃优先排序 + goalStatusMeta 状态映射。
import { describe, expect, it } from "vitest";
import { goalStatusMeta, rankCards } from "../lib/board";
import type { WorkspaceOverview } from "../lib/chat";

function card(path: string, opt: Partial<WorkspaceOverview> = {}): WorkspaceOverview {
  return {
    path,
    sessions: 0,
    running: 0,
    last_activity: 0,
    dirty: null,
    running_sessions: [],
    worktrees: [],
    goal: null,
    kanban: [],
    queued: 0,
    cron: 0,
    ...opt,
  };
}

describe("rankCards", () => {
  it("运行中 > 有排队 > 最近活动", () => {
    const list = [
      card("/idle-old", { last_activity: 100 }),
      card("/queued", { queued: 2, last_activity: 50 }),
      card("/running", { running: 1, last_activity: 10 }),
      card("/idle-new", { last_activity: 500 }),
    ];
    expect(rankCards(list).map((c) => c.path)).toEqual([
      "/running",
      "/queued",
      "/idle-new",
      "/idle-old",
    ]);
  });

  it("不改动原数组", () => {
    const list = [card("/b", { last_activity: 1 }), card("/a", { last_activity: 2 })];
    rankCards(list);
    expect(list.map((c) => c.path)).toEqual(["/b", "/a"]);
  });
});

describe("goalStatusMeta", () => {
  it("活态映射到中文文案与色调", () => {
    expect(goalStatusMeta("active")).toEqual({ label: "进行中", tone: "ok" });
    expect(goalStatusMeta("blocked")).toEqual({ label: "阻塞", tone: "warn" });
    expect(goalStatusMeta("budget_limited")).toEqual({ label: "预算触顶", tone: "warn" });
    expect(goalStatusMeta("paused")).toEqual({ label: "已暂停", tone: "dim" });
  });

  it("未知状态原样透出且 dim", () => {
    expect(goalStatusMeta("queued")).toEqual({ label: "queued", tone: "dim" });
  });
});
