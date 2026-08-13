import { render } from "solid-js/web";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { JSX } from "solid-js";

const h = vi.hoisted(() => ({
  on: vi.fn((_handler: (payload: unknown, topic?: string) => void) => vi.fn()),
  stream: vi.fn(),
  resync: new Set<() => void>(),
}));
h.stream.mockImplementation(() => ({ on: h.on }));

vi.mock("../lib/client", () => ({
  client: {
    stream: h.stream,
    onResync: (callback: () => void) => {
      h.resync.add(callback);
      return () => h.resync.delete(callback);
    },
  },
}));
vi.mock("@solidjs/router", () => ({
  A: (props: { href: string; class?: string; children?: JSX.Element }) => (
    <a href={props.href} class={props.class}>
      {props.children}
    </a>
  ),
}));
vi.mock("../lib/runtime", () => ({ isTauri: () => false }));
vi.mock("../components/bots/BotLibrary", () => ({ default: () => <div>library-content</div> }));
vi.mock("../components/bots/BotBuilder", () => ({ default: () => <div>builder-content</div> }));
vi.mock("../components/bots/BotCollaboration", () => ({
  default: () => <div>collaboration-content</div>,
}));
vi.mock("../components/bots/BotRoutines", () => ({ default: () => <div>routine-content</div> }));
vi.mock("../components/bots/BotRuns", () => ({ default: () => <div>runs-content</div> }));
vi.mock("../components/bots/BotRecovery", () => ({ default: () => <div>recovery-content</div> }));

import Bots from "./Bots";

afterEach(() => {
  document.body.innerHTML = "";
  h.stream.mockClear();
  h.on.mockClear();
  h.resync.clear();
});

describe("Bots product block", () => {
  it("明确 Bot Group 语义并在六个独立功能面板间切换", () => {
    const dispose = render(() => <Bots />, document.body);
    expect(document.body.textContent).toContain("Bot Group 表示多个 Bot 协作，不是多人聊天");
    expect(document.body.textContent).toContain("library-content");
    const collaboration = [...document.body.querySelectorAll("button")].find((button) =>
      button.textContent?.includes("Bot-to-Bot"),
    );
    collaboration?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(document.body.textContent).toContain("collaboration-content");
    expect(collaboration?.getAttribute("aria-selected")).toBe("true");
    expect(document.querySelector("[role='tabpanel']")?.getAttribute("aria-labelledby")).toBe(
      collaboration?.id,
    );
    dispose();
  });

  it("页签支持水平键盘导航并自动切换面板", () => {
    const dispose = render(() => <Bots />, document.body);
    const library = document.querySelector<HTMLButtonElement>("[role='tab'][aria-selected='true']");
    library?.focus();
    library?.dispatchEvent(
      new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true, cancelable: true }),
    );
    const build = document.querySelector<HTMLButtonElement>("#bots-tab-build");
    expect(document.activeElement).toBe(build);
    expect(build?.getAttribute("aria-selected")).toBe("true");
    expect(document.body.textContent).toContain("builder-content");
    dispose();
  });

  it("订阅 bots invalidation 和 resync，卸载时注销", () => {
    const dispose = render(() => <Bots />, document.body);
    expect(h.stream).toHaveBeenCalledWith("bots");
    expect(h.resync.size).toBe(1);
    dispose();
    expect(h.resync.size).toBe(0);
  });
});
