// agent diff 三态：RPC 失败与真空可区分（err 带原因），重试可恢复；并发慢响应不覆盖新帧。
import { afterEach, describe, expect, it, vi } from "vitest";
import { createAgentDiff, fetchAgentDiffFile, fetchAgentDiffStatus } from "./agent-diff";

const rpcMock = vi.hoisted(() => ({
  impl: (_method: string, _params?: unknown) =>
    Promise.reject(new Error("unexpected call")) as Promise<unknown>,
}));
vi.mock("./client", () => ({
  client: { rpc: (method: string, params?: unknown) => rpcMock.impl(method, params) },
}));

afterEach(() => {
  rpcMock.impl = () => Promise.reject(new Error("unexpected call"));
});

describe("fetchAgentDiffStatus", () => {
  it("成功返回 ok + entries", async () => {
    rpcMock.impl = () =>
      Promise.resolve([{ path: "a.ts", added: 1, deleted: 0, status: "modified" }]);
    const r = await fetchAgentDiffStatus("s1");
    expect(r).toEqual({
      state: "ok",
      entries: [{ path: "a.ts", added: 1, deleted: 0, status: "modified" }],
    });
  });

  it("真空（空数组）是 ok 态而非 err", async () => {
    rpcMock.impl = () => Promise.resolve([]);
    const r = await fetchAgentDiffStatus("s1");
    expect(r).toEqual({ state: "ok", entries: [] });
  });

  it("失败返回 err 且原因上屏（不再吞成 []）", async () => {
    rpcMock.impl = () => Promise.reject(new Error("session gone"));
    const r = await fetchAgentDiffStatus("s1");
    expect(r.state).toBe("err");
    expect(r.state === "err" && r.message).toContain("session gone");
  });
});

describe("fetchAgentDiffFile", () => {
  it("成功返回 ok + text", async () => {
    rpcMock.impl = () => Promise.resolve({ text: "+line" });
    const r = await fetchAgentDiffFile("s1", "a.ts");
    expect(r).toEqual({ state: "ok", text: "+line" });
  });

  it("失败返回 err 带原因（不再吞成空串）", async () => {
    rpcMock.impl = () => Promise.reject(new Error("io error"));
    const r = await fetchAgentDiffFile("s1", "a.ts");
    expect(r.state).toBe("err");
    expect(r.state === "err" && r.message).toContain("io error");
  });
});

describe("createAgentDiff", () => {
  it("初始 loading，首拉成功转 ok；失败转 err 后 reload 可重试恢复", async () => {
    let calls = 0;
    rpcMock.impl = () => {
      calls++;
      return calls === 1
        ? Promise.reject(new Error("net down"))
        : Promise.resolve([{ path: "a.ts", added: 2, deleted: 1, status: "modified" }]);
    };
    const store = createAgentDiff(() => "s1");
    expect(store.status().state).toBe("loading");

    await store.reload();
    expect(store.status().state).toBe("err");

    await store.reload(); // err 态重试
    const s = store.status();
    expect(s.state).toBe("ok");
    expect(s.state === "ok" && s.entries.length).toBe(1);
  });

  it("无活跃会话按真空处理（不发 RPC）", async () => {
    rpcMock.impl = () => Promise.reject(new Error("should not be called"));
    const store = createAgentDiff(() => "");
    await store.reload();
    expect(store.status()).toEqual({ state: "ok", entries: [] });
  });

  it("慢响应不覆盖更新的帧", async () => {
    let resolveSlow!: (v: unknown) => void;
    let calls = 0;
    rpcMock.impl = () => {
      calls++;
      return calls === 1
        ? new Promise((r) => (resolveSlow = r))
        : Promise.resolve([{ path: "b.ts", added: 1, deleted: 0, status: "created" }]);
    };
    const store = createAgentDiff(() => "s1");
    const p1 = store.reload();
    const p2 = store.reload(); // 后发起的先落地
    await p2;
    resolveSlow([]);
    await p1;
    const s = store.status();
    expect(s.state === "ok" && s.entries[0]?.path).toBe("b.ts");
  });
});
