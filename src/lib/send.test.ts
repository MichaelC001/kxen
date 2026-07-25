// 发送链路：sendMessage 失败不再静默吞错——气泡挂失败态 + flash 原因 + 点击重发原样带回。
import { createSignal } from "solid-js";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ContextItem } from "./chat";
import type { Item, MsgItem } from "./items";

const h = vi.hoisted(() => ({
  sendMessage: vi.fn(),
  ensureActiveSession: vi.fn(async () => "s1"),
  flashErr: vi.fn(),
}));

vi.mock("./chat", () => ({ sendMessage: h.sendMessage }));
vi.mock("./state", () => ({ ensureActiveSession: h.ensureActiveSession }));
vi.mock("./flash", () => ({ flashErr: h.flashErr }));

import { createSendFlow } from "./send";

function setup() {
  const [items, setItems] = createSignal<Item[]>([]);
  const [queue, setQueue] = createSignal<string[]>([]);
  let sid = "";
  const flow = createSendFlow({
    streaming: () => sid !== "",
    onStreamStart: (id) => {
      sid = id;
    },
    onStreamStop: (id) => {
      if (sid === id) sid = "";
    },
    setItems,
    setPendingQueue: setQueue,
    scroll: () => {},
  });
  return { flow, items, queue, streaming: () => sid !== "" };
}

beforeEach(() => {
  h.sendMessage.mockReset();
  h.ensureActiveSession.mockClear();
  h.flashErr.mockClear();
});

describe("发送链路失败态", () => {
  it("发送失败：气泡挂失败态 + flash 原因 + 收回 streaming", async () => {
    h.sendMessage.mockRejectedValueOnce(new Error("rpc timeout: send_message"));
    const s = setup();
    await s.flow.send("你好", [{ type: "file", path: "/a.ts" }], []);
    const bubble = s.items()[0] as MsgItem;
    expect(bubble.kind).toBe("msg");
    expect(bubble.sendError).toContain("rpc timeout");
    expect(h.flashErr).toHaveBeenCalledTimes(1);
    expect(s.streaming()).toBe(false);
  });

  it("点击重发：撤下失败气泡，原始 text/context/images 重新送达", async () => {
    h.sendMessage.mockRejectedValueOnce(new Error("boom")).mockResolvedValueOnce({});
    const s = setup();
    const ctx: ContextItem[] = [{ type: "file", path: "/a.ts" }];
    const imgs = [{ media_type: "image/png", data: "QUJD" }];
    await s.flow.send("hi", ctx, imgs);
    const failed = s.items()[0] as MsgItem;
    expect(failed.sendError).toBeTruthy();
    await s.flow.retry(failed);
    expect(h.sendMessage).toHaveBeenNthCalledWith(2, "s1", "hi", ctx, imgs);
    expect(s.items()).toHaveLength(1);
    const rebubble = s.items()[0] as MsgItem;
    expect(rebubble).not.toBe(failed);
    expect(rebubble.sendError).toBeUndefined();
    expect(rebubble.content).toBe("hi");
  });

  it("排队中的发送失败不清 streaming（当前 run 仍在跑）", async () => {
    h.sendMessage.mockResolvedValueOnce({});
    const s = setup();
    await s.flow.send("第一条", [], []);
    expect(s.streaming()).toBe(true);
    h.sendMessage.mockRejectedValueOnce(new Error("connection lost"));
    await s.flow.send("第二条", [], []);
    expect(s.streaming()).toBe(true);
    expect((s.items()[1] as MsgItem).sendError).toContain("connection lost");
  });

  it("发送成功且 queued 时进待发队列，气泡无失败态", async () => {
    h.sendMessage.mockResolvedValueOnce({ queued: true });
    const s = setup();
    await s.flow.send("排队", [], []);
    expect(s.queue()).toEqual(["排队"]);
    expect((s.items()[0] as MsgItem).sendError).toBeUndefined();
  });

  it("会话创建失败：flash 原因，不上屏气泡", async () => {
    h.ensureActiveSession.mockRejectedValueOnce(new Error("no workspace"));
    const s = setup();
    await s.flow.send("hi", [], []);
    expect(s.items()).toHaveLength(0);
    expect(h.flashErr).toHaveBeenCalledTimes(1);
  });
});
