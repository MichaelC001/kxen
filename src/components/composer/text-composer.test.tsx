// TextComposer 实测：原生键入 / IME Enter 守卫 / slash 任意位置 / 大粘贴折叠 / 图片 chip / 草稿隔离。
import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import { describe, expect, it, vi } from "vitest";
import { userEvent } from "@vitest/browser/context";
import TextComposer from "./TextComposer";
import { setActiveSessionId } from "../../lib/state";

// 测试环境无 WS 后端：命令清单 mock 成内建子集（slash 弹层数据源）
vi.mock("../../lib/chat", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../../lib/chat")>();
  return {
    ...orig,
    commandList: async () => [{ name: "doctor", description: "环境自检", kind: "builtin" }],
  };
});

function mount(onSend: (text: string) => void = () => {}) {
  const [tick, setTick] = createSignal(0);
  const dispose = render(
    () => (
      <TextComposer
        streaming={() => false}
        onSend={(t) => onSend(t)}
        onStop={() => {}}
        focusTick={tick}
      />
    ),
    document.body,
  );
  return { dispose, setTick, ta: () => document.querySelector<HTMLTextAreaElement>("textarea")! };
}

describe("TextComposer (webkit)", () => {
  it("原生键入上字 + Enter 发送", async () => {
    let sent = "";
    const { dispose, ta } = mount((t) => (sent = t));
    await new Promise((r) => setTimeout(r, 100));
    ta().focus();
    await userEvent.keyboard("hello composer");
    expect(ta().value).toBe("hello composer");
    await userEvent.keyboard("{Enter}");
    expect(sent).toBe("hello composer");
    expect(ta().value).toBe("");
    dispose();
  });

  it("IME 提交 Enter 不发送（compositionend 后 50ms 锁窗）", async () => {
    let sent = "";
    const { dispose, ta } = mount((t) => (sent = t));
    await new Promise((r) => setTimeout(r, 100));
    const el = ta();
    el.focus();
    await userEvent.keyboard("nihao");
    // Safari 顺序：compositionend 先，commit keydown 后（isComposing=false）——锁窗必须吞掉
    el.dispatchEvent(new CompositionEvent("compositionend", { data: "你好" }));
    el.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }),
    );
    expect(sent).toBe("");
    el.remove();
    dispose();
  });

  it("slash 任意位置触发弹层（空白前界）", async () => {
    const { dispose, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    ta().focus();
    await userEvent.keyboard("帮我 /doc");
    await new Promise((r) => setTimeout(r, 400));
    const popup = document.querySelector(".composer-popup");
    expect(popup).not.toBeNull();
    expect(popup?.textContent).toContain("/doctor");
    // 弹层必须落在视口内（镜像定位曾经量到页面原点，弹出屏幕外）
    const rect = popup!.getBoundingClientRect();
    expect(rect.bottom).toBeGreaterThan(0);
    expect(rect.top).toBeLessThan(window.innerHeight);
    dispose();
  });

  it("路径形态的 / 不触发（前界非空白）", async () => {
    const { dispose, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    ta().focus();
    await userEvent.keyboard("src/comp");
    await new Promise((r) => setTimeout(r, 400));
    expect(document.querySelector(".composer-popup")).toBeNull();
    dispose();
  });

  it("大粘贴折叠为占位，发送时展开全文", async () => {
    let sent = "";
    const { dispose, ta } = mount((t) => (sent = t));
    await new Promise((r) => setTimeout(r, 100));
    const el = ta();
    el.focus();
    const big = Array.from({ length: 30 }, (_, i) => `line ${i + 1}`).join("\n");
    const dt = new DataTransfer();
    dt.setData("text/plain", big);
    el.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }),
    );
    await new Promise((r) => setTimeout(r, 50));
    expect(el.value).toBe("[Pasted #1]");
    await userEvent.keyboard("{Enter}");
    expect(sent).toBe(big);
    dispose();
  });

  it("图片粘贴进框外 row chip", async () => {
    const { dispose, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    const file = new File(["x"], "a.png", { type: "image/png" });
    const dt = new DataTransfer();
    dt.items.add(file);
    ta().dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }),
    );
    await new Promise((r) => setTimeout(r, 100));
    expect(ta().value).toBe("");
    expect(document.querySelector(".composer-card")?.textContent).toContain("图片 png");
    dispose();
  });

  it("每会话草稿隔离恢复", async () => {
    const { dispose, setTick, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    setActiveSessionId("s1");
    setTick(1);
    await new Promise((r) => setTimeout(r, 50));
    ta().focus();
    await userEvent.keyboard("hello draft");
    await new Promise((r) => setTimeout(r, 100));
    setActiveSessionId("s2");
    setTick(2);
    await new Promise((r) => setTimeout(r, 100));
    expect(ta().value).toBe("");
    setActiveSessionId("s1");
    setTick(3);
    await new Promise((r) => setTimeout(r, 100));
    expect(ta().value).toBe("hello draft");
    dispose();
  });
});
