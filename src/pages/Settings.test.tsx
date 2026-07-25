// Settings 通用区回归：「运行中发送」乐观更新在 RPC 失败时必须回滚到旧值并 flashErr，
// 不留与后端不一致的假状态；saved 死代码已删（页面不再有常驻提示条容器）。
import { render } from "solid-js/web";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { JSX } from "solid-js";

const h = vi.hoisted(() => ({
  cfg: vi.fn(async () => ({ roles: {}, send_when_running: "queue" }) as unknown),
  rpc: vi.fn((_method: string, _params?: unknown) => Promise.resolve({}) as Promise<unknown>),
}));

vi.mock("../lib/chat", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../lib/chat")>();
  return { ...orig, configGet: h.cfg };
});

vi.mock("../lib/client", () => ({ client: { rpc: h.rpc } }));

// <A> 依赖 Router 上下文：测试无路由装配，桩成普通锚
vi.mock("@solidjs/router", () => ({
  A: (props: { href: string; class?: string; children?: JSX.Element }) => (
    <a href={props.href} class={props.class}>
      {props.children}
    </a>
  ),
}));

import Settings from "./Settings";
import { flash } from "../lib/flash";

const flush = () => new Promise((r) => setTimeout(r, 0));

function btnByText(text: string): HTMLButtonElement {
  const found = [...document.body.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) => b.textContent === text,
  );
  if (!found) throw new Error(`button not found: ${text}`);
  return found;
}

beforeEach(() => {
  h.cfg.mockResolvedValue({ roles: {}, send_when_running: "queue" });
  h.rpc.mockReset();
  h.rpc.mockResolvedValue({});
});

afterEach(() => {
  document.body.innerHTML = "";
  for (const m of flash.msgs()) flash.dismiss(m.id);
  vi.clearAllMocks();
});

describe("Settings 运行中发送", () => {
  it("RPC 失败：回滚到旧策略并 flashErr，不留假状态", async () => {
    h.rpc.mockImplementation((method: string) =>
      method === "config.set_send_policy"
        ? Promise.reject(new Error("disk read-only"))
        : Promise.resolve({}),
    );
    const dispose = render(() => <Settings />, document.body);
    await flush();

    btnByText("打断").click();
    await vi.waitFor(() => {
      expect(h.rpc).toHaveBeenCalledWith("config.set_send_policy", { policy: "interrupt" });
      const err = flash.msgs().find((m) => m.kind === "err");
      expect(err?.text).toContain("保存失败");
      expect(err?.text).toContain("disk read-only");
    });
    // 回滚：高亮回到「排队」
    await vi.waitFor(() => expect(btnByText("排队").className).toContain("border-[var(--accent)]"));
    expect(btnByText("打断").className).not.toContain("border-[var(--accent)]");
    dispose();
  });

  it("RPC 成功：切到打断且不报错", async () => {
    const dispose = render(() => <Settings />, document.body);
    await flush();
    btnByText("打断").click();
    await vi.waitFor(() => expect(btnByText("打断").className).toContain("border-[var(--accent)]"));
    expect(flash.msgs().some((m) => m.kind === "err")).toBe(false);
    dispose();
  });
});
