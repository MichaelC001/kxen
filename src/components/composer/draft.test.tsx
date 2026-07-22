// 每会话草稿实测（B6）：s1 输入 -> 切 s2 空 -> 切回 s1 恢复。
import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import { describe, expect, it } from "vitest";
import { userEvent } from "@vitest/browser/context";
import LexicalComposer from "./LexicalComposer";
import { setActiveSessionId } from "../../lib/state";

describe("per-session draft (webkit)", () => {
  it("切会话草稿隔离恢复", async () => {
    const [tick, setTick] = createSignal(0);
    const dispose = render(
      () => (
        <LexicalComposer
          streaming={() => false}
          onSend={() => {}}
          onStop={() => {}}
          focusTick={tick}
        />
      ),
      document.body,
    );
    await new Promise((r) => setTimeout(r, 100));
    const root = document.querySelector<HTMLElement>(".editor-root")!;
    setActiveSessionId("s1");
    setTick(1);
    root.focus();
    await userEvent.keyboard("hello draft");
    await new Promise((r) => setTimeout(r, 150));
    expect(root.textContent).toContain("hello draft");

    setActiveSessionId("s2");
    setTick(2);
    await new Promise((r) => setTimeout(r, 100));
    expect(root.textContent ?? "").not.toContain("hello draft");

    setActiveSessionId("s1");
    setTick(3);
    await new Promise((r) => setTimeout(r, 100));
    expect(root.textContent).toContain("hello draft");
    dispose();
  });
});
