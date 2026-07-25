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

function listCalls(): number {
  return h.rpc.mock.calls.filter((c) => c[0] === "notifications.list").length;
}

afterEach(() => {
  document.body.innerHTML = "";
  h.rpc.mockClear();
  h.resync.clear();
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
