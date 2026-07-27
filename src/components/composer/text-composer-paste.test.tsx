// TextComposer 粘贴/附件/草稿标注/token 估算实测（350 行门禁从 text-composer.test.tsx 拆出）：
// 图片 chip 释放 images、混合剪贴板文本不丢、小粘贴 CRLF 归一、截断标注剥除、估算分级随 ctx 窗。
import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import "../../styles.css";
import { afterEach, describe, expect, it, vi } from "vitest";
import { userEvent } from "@vitest/browser/context";
import TextComposer from "./TextComposer";
import { setActiveSessionId } from "../../lib/state";
import { clearDraft } from "../../lib/drafts";

vi.mock("../../lib/chat", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../../lib/chat")>();
  return {
    ...orig,
    commandList: async () => [],
    // token 估算分级数据源：固定当前模型，配合下方 catalog 的 ctx=100
    currentModel: async () => ({ provider: "xai", model: "grok-1" }),
    sessionList: async () => [],
  };
});

vi.mock("../../lib/models", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../../lib/models")>();
  return {
    ...orig,
    modelsCatalog: async () => [
      {
        provider: "xai",
        provider_name: "xAI",
        fetched_at: 0,
        source: "test",
        models: [
          {
            id: "grok-1",
            name: "Grok 1",
            family: "grok",
            reasoning: false,
            tool_call: true,
            attachment: false,
            modalities_in: ["text"],
            context: 100,
            output: 4096,
          },
        ],
      },
    ],
  };
});

afterEach(() => {
  clearDraft("");
  clearDraft("s9");
  localStorage.removeItem("kxen:draft:s9");
  setActiveSessionId("");
  // 失败用例没跑到 dispose 时清场：残留 composer 会让下一个用例的 ta() 抓到旧 textarea
  document.body.innerHTML = "";
});

function mount(onSend: (text: string, images?: Array<unknown>) => void = () => {}) {
  const [tick, setTick] = createSignal(0);
  const dispose = render(
    () => (
      <TextComposer
        streaming={() => false}
        onSend={(t, _c, imgs) => onSend(t, imgs)}
        onStop={() => {}}
        focusTick={tick}
      />
    ),
    document.body,
  );
  return { dispose, setTick, ta: () => document.querySelector<HTMLTextAreaElement>("textarea")! };
}

function pasteFile(ta: HTMLTextAreaElement, dt: DataTransfer) {
  ta.dispatchEvent(
    new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }),
  );
}

describe("TextComposer 粘贴/附件 (webkit)", () => {
  it("图片 chip 移除后发送不再携带图片数据（images 随 chip 释放）", async () => {
    let imgs: unknown[] = [];
    const { dispose, ta } = mount((_t, i) => (imgs = i ?? []));
    await new Promise((r) => setTimeout(r, 100));
    const dt = new DataTransfer();
    dt.items.add(new File(["x"], "a.png", { type: "image/png" }));
    pasteFile(ta(), dt);
    await vi.waitFor(() =>
      expect(document.querySelector(".composer-card")?.textContent).toContain("图片 png"),
    );
    const chipX = [...document.querySelectorAll<HTMLElement>(".composer-card button")].find((b) =>
      b.parentElement?.textContent?.includes("图片 png"),
    )!;
    chipX.click();
    await new Promise((r) => setTimeout(r, 30));
    expect(document.querySelector(".composer-card")?.textContent).not.toContain("图片 png");
    ta().focus();
    await userEvent.keyboard("hi{Enter}");
    expect(imgs.length).toBe(0);
    dispose();
  });

  it("混合剪贴板（图片+文本）：文本随附件一起上屏，不被 files 吞掉", async () => {
    const { dispose, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    const dt = new DataTransfer();
    dt.items.add(new File(["x"], "a.png", { type: "image/png" }));
    dt.setData("text/plain", "附图说明");
    pasteFile(ta(), dt);
    expect(ta().value).toBe("附图说明");
    await vi.waitFor(() =>
      expect(document.querySelector(".composer-card")?.textContent).toContain("图片 png"),
    );
    dispose();
  });

  it("小粘贴 CRLF 归一为 LF", async () => {
    const { dispose, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    const dt = new DataTransfer();
    dt.setData("text/plain", "a\r\nb");
    pasteFile(ta(), dt);
    await new Promise((r) => setTimeout(r, 50));
    expect(ta().value).toBe("a\nb");
    dispose();
  });

  it("冷启动恢复的截断草稿剥掉标注（标注是存储层告示，发出即污染 prompt）", async () => {
    localStorage.setItem("kxen:draft:s9", "半截草稿\n[草稿过长，已截断]");
    const { dispose, setTick, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    setActiveSessionId("s9");
    setTick(1);
    await new Promise((r) => setTimeout(r, 100));
    expect(ta().value).toBe("半截草稿");
    dispose();
  });

  it("token 估算分级跟当前模型 ctx 窗（mock ctx=100：80 警 / 95 险）", async () => {
    const { dispose, ta } = mount();
    await new Promise((r) => setTimeout(r, 150));
    const span = () => document.querySelector<HTMLElement>(".tabular-nums")!;
    const type = (v: string) => {
      const el = ta();
      el.value = v;
      el.dispatchEvent(new InputEvent("input", { bubbles: true }));
    };
    type("x".repeat(340)); // 85 tok > 80（窗的 80%）
    await new Promise((r) => setTimeout(r, 30));
    expect(span().className).toContain("--warn");
    type("x".repeat(420)); // 105 tok > 95（窗的 95%）
    await new Promise((r) => setTimeout(r, 30));
    expect(span().className).toContain("--err");
    dispose();
  });
});
