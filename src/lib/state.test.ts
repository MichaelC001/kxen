// deleteSession 善后（P0-6）：活跃会话被删后 activeSessionId 不得悬死指向死会话。
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SessionMeta } from "./chat";
import type { AgentActivity } from "./team";

const mocks = vi.hoisted(() => ({
  sessionDelete: vi.fn<(id: string) => Promise<void>>(() => Promise.resolve()),
  sessionList: vi.fn<() => Promise<SessionMeta[]>>(() => Promise.resolve([])),
  sessionCreate: vi.fn(),
  rpc: vi.fn(() => Promise.resolve()),
  agentsList: vi.fn<(sid: string) => Promise<AgentActivity[]>>(() => Promise.resolve([])),
  streamHandlers: new Set<(p: unknown) => void>(),
  resyncHandlers: new Set<() => void>(),
}));
vi.mock("./chat", () => ({
  sessionDelete: mocks.sessionDelete,
  sessionList: mocks.sessionList,
  sessionCreate: mocks.sessionCreate,
}));
vi.mock("./client", () => ({
  client: {
    rpc: mocks.rpc,
    stream: () => ({
      on: (cb: (p: unknown) => void) => {
        mocks.streamHandlers.add(cb);
        return () => mocks.streamHandlers.delete(cb);
      },
    }),
    onResync: (cb: () => void) => {
      mocks.resyncHandlers.add(cb);
      return () => mocks.resyncHandlers.delete(cb);
    },
  },
}));
vi.mock("./team", () => ({ agentsList: mocks.agentsList }));
vi.mock("./session-model", () => ({ applyDraftModel: vi.fn(() => Promise.resolve()) }));
vi.mock("./drafts", () => ({ migrateNewDraft: vi.fn() }));

import {
  activeSessionId,
  agents,
  deleteSession,
  ensureActiveSession,
  mountSessionEvents,
  refreshAgents,
  refreshSessions,
  sessions,
  setActiveSessionId,
  setAgents,
  setSessions,
} from "./state";

function meta(id: string, directory: string): SessionMeta {
  return { id, title: id, directory, created_at: 0, updated_at: 0 };
}

beforeEach(() => {
  mocks.sessionDelete.mockClear();
  mocks.sessionList.mockReset().mockResolvedValue([]);
  mocks.agentsList.mockReset().mockResolvedValue([]);
  mocks.streamHandlers.clear();
  mocks.resyncHandlers.clear();
  setSessions([]);
  setAgents([]);
  setActiveSessionId("");
});

describe("deleteSession 善后切换", () => {
  it("删活跃会话：切到同目录下一条", async () => {
    setSessions([meta("a", "/p"), meta("b", "/p"), meta("c", "/q")]);
    setActiveSessionId("a");
    mocks.sessionList.mockResolvedValue([meta("b", "/p"), meta("c", "/q")]);
    await deleteSession("a");
    expect(mocks.sessionDelete).toHaveBeenCalledWith("a");
    expect(activeSessionId()).toBe("b");
  });

  it("同目录无下一条：切列表首条", async () => {
    setSessions([meta("a", "/p"), meta("c", "/q")]);
    setActiveSessionId("a");
    mocks.sessionList.mockResolvedValue([meta("c", "/q")]);
    await deleteSession("a");
    expect(activeSessionId()).toBe("c");
  });

  it("列表删空：回草稿态（activeSessionId 置空）", async () => {
    setSessions([meta("a", "/p")]);
    setActiveSessionId("a");
    mocks.sessionList.mockResolvedValue([]);
    await deleteSession("a");
    expect(activeSessionId()).toBe("");
  });

  it("删非活跃会话：活跃会话不动", async () => {
    setSessions([meta("a", "/p"), meta("b", "/p")]);
    setActiveSessionId("a");
    mocks.sessionList.mockResolvedValue([meta("a", "/p")]);
    await deleteSession("b");
    expect(activeSessionId()).toBe("a");
  });

  it("删除失败：错误上抛不静默（调用方负责 flashErr）", async () => {
    setSessions([meta("a", "/p")]);
    setActiveSessionId("a");
    mocks.sessionDelete.mockRejectedValueOnce(new Error("io boom"));
    await expect(deleteSession("a")).rejects.toThrow("io boom");
  });
});

describe("ensureActiveSession 并发去重", () => {
  it("并发首发共享同一次创建：只建一个会话，两路拿到同一 id", async () => {
    mocks.sessionCreate.mockReset();
    let release!: (m: SessionMeta) => void;
    mocks.sessionCreate.mockImplementationOnce(
      () =>
        new Promise<SessionMeta>((r) => {
          release = r;
        }),
    );
    mocks.sessionList.mockResolvedValue([meta("s-new", "/p")]);
    const p1 = ensureActiveSession();
    const p2 = ensureActiveSession();
    release(meta("s-new", "/p"));
    const [a, b] = await Promise.all([p1, p2]);
    expect(a).toBe("s-new");
    expect(b).toBe("s-new");
    expect(mocks.sessionCreate).toHaveBeenCalledTimes(1);
    expect(activeSessionId()).toBe("s-new");
  });
});

describe("refreshAgents / refreshSessions 引用稳定合并", () => {
  function agent(name: string, status: AgentActivity["status"]): AgentActivity {
    return { name, kind: "teammate", model: { provider: "p", model: "m" }, status, started_at: 0 };
  }

  it("轮询内容无变化：item 与数组引用都稳定（<For> 不重建，同引用 set 不传播）", async () => {
    setActiveSessionId("s1");
    mocks.agentsList.mockResolvedValue([agent("w", "working")]);
    await refreshAgents();
    const first = agents();
    mocks.agentsList.mockResolvedValue([agent("w", "working")]); // 全新对象同内容
    await refreshAgents();
    expect(agents()).toBe(first);
    expect(agents()[0]).toBe(first[0]);
  });

  it("仅状态变化项换新引用，其余复用旧对象", async () => {
    setActiveSessionId("s1");
    mocks.agentsList.mockResolvedValue([agent("w", "working"), agent("r", "working")]);
    await refreshAgents();
    const [w1, r1] = agents();
    mocks.agentsList.mockResolvedValue([agent("w", "working"), agent("r", "done")]);
    await refreshAgents();
    expect(agents()[0]).toBe(w1);
    expect(agents()[1]).not.toBe(r1);
    expect(agents()[1]!.status).toBe("done");
  });

  it("refreshSessions 同款保引用（SessionRow 行内编辑态不被 refresh 销毁）", async () => {
    mocks.sessionList.mockResolvedValue([meta("a", "/p")]);
    await refreshSessions();
    const first = sessions();
    mocks.sessionList.mockResolvedValue([meta("a", "/p")]);
    await refreshSessions();
    expect(sessions()).toBe(first);
    mocks.sessionList.mockResolvedValue([{ ...meta("a", "/p"), title: "改名" }]);
    await refreshSessions();
    expect(sessions()[0]).not.toBe(first[0]);
    expect(sessions()[0]!.title).toBe("改名");
  });
});

describe("mountSessionEvents 事件驱动刷新", () => {
  const fireStream = () => {
    for (const cb of mocks.streamHandlers) cb({ session_id: "a", running: true });
  };
  const fireResync = () => {
    for (const cb of mocks.resyncHandlers) cb();
  };

  it("session.update 连发帧去抖为一次 refreshSessions", async () => {
    vi.useFakeTimers();
    const un = mountSessionEvents();
    fireStream();
    fireStream();
    expect(mocks.sessionList).not.toHaveBeenCalled(); // 去抖窗口内不落
    await vi.advanceTimersByTimeAsync(300);
    expect(mocks.sessionList).toHaveBeenCalledTimes(1);
    un();
    vi.useRealTimers();
  });

  it("resync 信号同样触发重拉（断线窗口的 run 存亡对账）", async () => {
    vi.useFakeTimers();
    const un = mountSessionEvents();
    fireResync();
    await vi.advanceTimersByTimeAsync(300);
    expect(mocks.sessionList).toHaveBeenCalledTimes(1);
    un();
    vi.useRealTimers();
  });

  it("注销后事件不再触发重拉", async () => {
    vi.useFakeTimers();
    const un = mountSessionEvents();
    un();
    fireStream();
    await vi.advanceTimersByTimeAsync(300);
    expect(mocks.sessionList).not.toHaveBeenCalled();
    vi.useRealTimers();
  });
});
