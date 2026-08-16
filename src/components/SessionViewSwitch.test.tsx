// Chat / Trajectory 双视图切换实测：标签切换显隐、Chat 保持挂载、切回恢复滚动锚点、
// requestInspectTool 联动切视图并给出定位目标。
import { render } from "solid-js/web";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { StoredMessage } from "../lib/chat";

vi.mock("../lib/chat", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../lib/chat")>();
  return { ...orig, sessionMessages: async () => [] as StoredMessage[] };
});
vi.mock("./Markdown", () => ({ default: (p: { text: string }) => <div>{p.text}</div> }));

import SessionViewSwitch from "./SessionViewSwitch";
import {
  inspectTarget,
  registerChatList,
  requestInspectTool,
  sessionView,
  switchSessionView,
} from "../lib/session-view";

function mount(sessionId = "s1") {
  return render(
    () => (
      <SessionViewSwitch sessionId={() => sessionId} streaming={() => false}>
        <div data-testid="chat-body">Chat 内容</div>
      </SessionViewSwitch>
    ),
    document.body,
  );
}

const tab = (name: string) => document.body.querySelector(`[data-testid='view-tab-${name}']`)!;

afterEach(() => {
  document.body.innerHTML = "";
  registerChatList(undefined);
  switchSessionView("chat");
});

describe("SessionViewSwitch 双视图", () => {
  it("默认 Chat 可见；切到 Trajectory 后 Chat 保持挂载仅隐藏，切回恢复", async () => {
    const dispose = mount();
    expect(document.body.textContent).toContain("Chat 内容");
    expect(document.body.querySelector("[data-testid='chat-body']")).toBeTruthy();
    tab("trajectory").dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(sessionView()).toBe("trajectory");
    // Chat 仍挂载（display:none），阅读位置状态存活
    const chatWrap = document.body.querySelector("[data-testid='chat-body']")!.parentElement!;
    expect(chatWrap.classList.contains("hidden")).toBe(true);
    // Trajectory 按需分包，首次激活需等 lazy chunk 加载
    await vi.waitFor(
      () => expect(document.body.querySelector("[data-testid='trajectory-view']")).toBeTruthy(),
      { timeout: 15000 },
    );
    tab("chat").dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(sessionView()).toBe("chat");
    expect(chatWrap.classList.contains("hidden")).toBe(false);
    dispose();
  });

  it("切走记滚动锚点，切回恢复 scrollTop", async () => {
    const dispose = mount();
    const list = document.createElement("div");
    registerChatList(list);
    Object.defineProperty(list, "scrollTop", { value: 432, writable: true });
    switchSessionView("trajectory");
    list.scrollTop = 0; // 隐藏期间被扰动
    switchSessionView("chat");
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(list.scrollTop).toBe(432);
    dispose();
  });

  it("requestInspectTool：有落盘定位才切视图并给出目标；流式条目（无定位）忽略", () => {
    requestInspectTool({ kind: "tool", name: "read", call: "a.ts" });
    expect(sessionView()).toBe("chat");
    expect(inspectTarget()).toBeNull();
    requestInspectTool({ kind: "tool", name: "read", call: "a.ts", messageId: "m2", partIndex: 1 });
    expect(sessionView()).toBe("trajectory");
    expect(inspectTarget()).toEqual({ messageId: "m2", partIndex: 1 });
  });

  it("草稿态（无 session）不出标签栏", () => {
    const dispose = mount("");
    expect(document.body.querySelector("[data-testid='view-tab-chat']")).toBeNull();
    dispose();
  });
});
