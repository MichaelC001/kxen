// NotificationCenter resync 自愈：bus lag 丢帧后服务端下发 resync，不等轮询立即重拉通知列表。
import { render } from "solid-js/web";
import { afterEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  rpc: vi.fn(async (_method: string) => [] as unknown[]),
  resync: new Set<() => void>(),
}));

vi.mock("../lib/client", () => ({
  client: {
    rpc: h.rpc,
    onResync: (cb: () => void) => {
      h.resync.add(cb);
      return () => h.resync.delete(cb);
    },
  },
}));

import NotificationCenter from "./NotificationCenter";
import { activeSessionId, setActiveSessionId, setSessions } from "../lib/state";

function listCalls(): number {
  return h.rpc.mock.calls.filter((c) => c[0] === "notifications.list").length;
}

afterEach(() => {
  document.body.innerHTML = "";
  h.rpc.mockClear();
  h.rpc.mockImplementation(async () => []);
  h.resync.clear();
  setSessions([]);
  setActiveSessionId("");
});

describe("NotificationCenter resync 自愈", () => {
  it("resync 信号触发重拉，卸载后注销回调", async () => {
    const dispose = render(() => <NotificationCenter />, document.body);
    await new Promise((r) => setTimeout(r, 0));
    expect(listCalls()).toBe(1); // onMount 首拉
    expect(h.resync.size).toBe(1);
    for (const cb of h.resync) cb();
    await new Promise((r) => setTimeout(r, 0));
    expect(listCalls()).toBe(2);
    dispose();
    expect(h.resync.size).toBe(0);
  });
});

describe("NotificationCenter 条目跳转", () => {
  it("带来源会话的条目点击切到该会话，无 session_id 的条目不可点", async () => {
    h.rpc.mockImplementation(async (method: string) =>
      method === "notifications.list"
        ? [
            { at: Date.now(), text: "teammate a: 已完成", session_id: "s9" },
            { at: Date.now(), text: "系统级通知", session_id: null },
          ]
        : [],
    );
    setSessions([{ id: "s9", title: "t9", directory: "/tmp", created_at: 0, updated_at: 0 }]);
    setActiveSessionId("s1");
    const dispose = render(() => <NotificationCenter />, document.body);
    await new Promise((r) => setTimeout(r, 0));
    (document.querySelector('button[title="通知中心"]') as HTMLButtonElement).click();
    await new Promise((r) => setTimeout(r, 0));
    const jumpBtns = document.querySelectorAll('button[title="跳到来源会话"]');
    expect(jumpBtns.length).toBe(1); // 仅带 session_id 的一条可点
    expect(jumpBtns[0]?.textContent).toContain("teammate a: 已完成");
    (jumpBtns[0] as HTMLButtonElement).click();
    expect(activeSessionId()).toBe("s9");
    dispose();
  });
});
