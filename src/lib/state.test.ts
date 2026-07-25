// deleteSession 善后（P0-6）：活跃会话被删后 activeSessionId 不得悬死指向死会话。
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SessionMeta } from "./chat";

const mocks = vi.hoisted(() => ({
  sessionDelete: vi.fn<(id: string) => Promise<void>>(() => Promise.resolve()),
  sessionList: vi.fn<() => Promise<SessionMeta[]>>(() => Promise.resolve([])),
  sessionCreate: vi.fn(),
  rpc: vi.fn(() => Promise.resolve()),
}));
vi.mock("./chat", () => ({
  sessionDelete: mocks.sessionDelete,
  sessionList: mocks.sessionList,
  sessionCreate: mocks.sessionCreate,
}));
vi.mock("./client", () => ({ client: { rpc: mocks.rpc } }));
vi.mock("./team", () => ({ agentsList: vi.fn(() => Promise.resolve([])) }));
vi.mock("./session-model", () => ({ applyDraftModel: vi.fn(() => Promise.resolve()) }));
vi.mock("./drafts", () => ({ migrateNewDraft: vi.fn() }));

import { activeSessionId, deleteSession, setActiveSessionId, setSessions } from "./state";

function meta(id: string, directory: string): SessionMeta {
  return { id, title: id, directory, created_at: 0, updated_at: 0 };
}

beforeEach(() => {
  mocks.sessionDelete.mockClear();
  mocks.sessionList.mockReset().mockResolvedValue([]);
  setSessions([]);
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
