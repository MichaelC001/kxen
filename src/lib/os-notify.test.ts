// OS 通知点击回跳：带来源会话的点击切到该会话；会话已删 flashErr 不悬空切换。
import { afterEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  handler: null as ((e: { payload: string }) => void) | null,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_event: string, cb: (e: { payload: string }) => void) => {
    h.handler = cb;
    return () => {
      h.handler = null;
    };
  }),
}));

vi.mock("./client", () => ({
  client: {
    rpc: vi.fn(async () => undefined),
  },
}));

import { mountOsNotificationJump } from "./os-notify";
import { activeSessionId, setActiveSessionId, setSessions } from "./state";
import { flash } from "./flash";

afterEach(() => {
  setSessions([]);
  setActiveSessionId("");
  for (const m of flash.msgs()) flash.dismiss(m.id);
  h.handler = null;
});

describe("os-notify 点击回跳", () => {
  it("点击载荷的会话存在：切到该会话；卸载注销 listen", async () => {
    setSessions([{ id: "s9", title: "t9", directory: "/tmp", created_at: 0, updated_at: 0 }]);
    const un = await mountOsNotificationJump();
    expect(h.handler).not.toBeNull();
    h.handler?.({ payload: "s9" });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(activeSessionId()).toBe("s9");
    un();
    expect(h.handler).toBeNull();
  });

  it("来源会话已删除：flashErr，不悬空切换", async () => {
    setSessions([]);
    setActiveSessionId("s1");
    await mountOsNotificationJump();
    h.handler?.({ payload: "ghost" });
    expect(activeSessionId()).toBe("s1");
    expect(flash.msgs().some((m) => m.kind === "err" && m.text.includes("来源会话已删除"))).toBe(
      true,
    );
  });
});
