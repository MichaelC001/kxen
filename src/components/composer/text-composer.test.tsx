// TextComposer 实测：原生键入 / IME 守卫（发送 + 弹层）/ slash 任意位置 / 行首与全角触发 /
// 弹层 apply 定界与关闭 / 大粘贴折叠 / 图片 chip / 草稿隔离。
import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import "../../styles.css";
import { afterEach, describe, expect, it, vi } from "vitest";
import { userEvent } from "@vitest/browser/context";
import TextComposer from "./TextComposer";
import { activeSessionId, ensureActiveSession, setActiveSessionId } from "../../lib/state";
import { clearDraft, getDraft } from "../../lib/drafts";

// 测试环境无 WS 后端：命令清单 mock 成内建子集（slash 弹层数据源）；
// session.create/list mock 成本地内存（首发落库路径走真实 ensureActiveSession）
const chatMock = vi.hoisted(() => {
  interface CreatedMeta {
    id: string;
    title: string;
    directory: string;
    created_at: number;
    updated_at: number;
  }
  function meta(): CreatedMeta {
    return { id: chatMock.createdId, title: "", directory: "", created_at: 0, updated_at: 0 };
  }
  const chatMock = {
    createdId: "s-created",
    deferred: false,
    resolvers: [] as Array<(m: CreatedMeta) => void>,
    meta,
  };
  return chatMock;
});

vi.mock("../../lib/chat", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../../lib/chat")>();
  return {
    ...orig,
    commandList: async () => [
      { name: "doctor", description: "环境自检", kind: "builtin" },
      {
        name: "ultracode",
        description: "大任务模式：分解 -> workflow 并行实现 -> 集成验证",
        kind: "builtin",
        argument_hint: "<实现任务>",
      },
    ],
    sessionList: async () => [],
    fsComplete: async (query: string) =>
      [
        { path: "src/App.tsx", kind: "file" },
        { path: "src/components", kind: "dir" },
      ].filter((e) => e.path.toLowerCase().includes(query.toLowerCase())),
    sessionCreate: async () => {
      if (!chatMock.deferred) return chatMock.meta();
      return new Promise((res) => chatMock.resolvers.push(res));
    },
  };
});

afterEach(() => {
  chatMock.deferred = false;
  chatMock.resolvers.length = 0;
  clearDraft("");
  clearDraft("s-created");
  setActiveSessionId("");
  // 失败用例没跑到 dispose 时清场：残留 composer 会让下一个用例的 ta() 抓到旧 textarea
  document.body.innerHTML = "";
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

  it("slash 长描述不挤没命令名（flex 饿死回归）", async () => {
    const { dispose, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    ta().focus();
    await userEvent.keyboard("/ultra");
    await new Promise((r) => setTimeout(r, 400));
    const rows = [...document.querySelectorAll(".composer-popup button")];
    const ultra = rows.find((r) => r.textContent?.includes("大任务模式"));
    expect(ultra).toBeTruthy();
    const label = ultra!.querySelector("span") as HTMLElement;
    expect(label.textContent).toContain("/ultracode");
    // 两行布局下 label 独占一行（曾经的单行 flex 布局会把 label 饿成 0 宽）
    const rowWidth = (ultra as HTMLElement).offsetWidth;
    expect(label.offsetWidth).toBeGreaterThan(rowWidth * 0.5);
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

  it("行首触发：换行后的 / 弹层", async () => {
    const { dispose, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    ta().focus();
    await userEvent.keyboard("第一行{Shift>}{Enter}{/Shift}/doc");
    await new Promise((r) => setTimeout(r, 400));
    const popup = document.querySelector(".composer-popup");
    expect(popup).not.toBeNull();
    expect(popup?.textContent).toContain("/doctor");
    dispose();
  });

  it("全角边界触发：（ 后的 / 弹层", async () => {
    const { dispose, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    ta().focus();
    await userEvent.keyboard("（/doc");
    await new Promise((r) => setTimeout(r, 400));
    expect(document.querySelector(".composer-popup")?.textContent).toContain("/doctor");
    dispose();
  });

  it("行首 @ 触发，Enter apply 成 chip 且文本定界干净", async () => {
    const { dispose, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    ta().focus();
    await userEvent.keyboard("hi{Shift>}{Enter}{/Shift}@App");
    await new Promise((r) => setTimeout(r, 400));
    expect(document.querySelector(".composer-popup")?.textContent).toContain("src/App.tsx");
    ta().dispatchEvent(
      new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }),
    );
    expect(ta().value).toBe("hi\n");
    expect(document.querySelector(".composer-card")?.textContent).toContain("App.tsx");
    expect(document.querySelector(".composer-popup")).toBeNull();
    dispose();
  });

  it("IME 组字中弹层不劫持 Enter/方向键", async () => {
    const { dispose, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    const el = ta();
    el.focus();
    // query "d" 命中 doctor + ultracode 两条：单条弹层方向键取模恒在 0 号位，没法断言导航
    await userEvent.keyboard("/d");
    await new Promise((r) => setTimeout(r, 400));
    expect(document.querySelector(".composer-popup")).not.toBeNull();
    // Safari 顺序：compositionend 先（进 50ms 锁窗），commit keydown 后（isComposing=false）
    el.dispatchEvent(new CompositionEvent("compositionend", { data: "你" }));
    el.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }),
    );
    expect(el.value).toBe("/d"); // apply 被放行，没有发生
    expect(document.querySelector(".composer-popup")).not.toBeNull();
    // 锁窗内方向键同样归 IME 候选窗：选中项不动
    const firstBtn = document.querySelector<HTMLElement>(".composer-popup button")!;
    el.dispatchEvent(
      new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true, cancelable: true }),
    );
    expect(firstBtn.classList.contains("bg-[var(--bg-overlay)]")).toBe(true);
    // 锁窗过后弹层导航恢复
    await new Promise((r) => setTimeout(r, 60));
    el.dispatchEvent(
      new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true, cancelable: true }),
    );
    expect(firstBtn.classList.contains("bg-[var(--bg-overlay)]")).toBe(false);
    dispose();
  });

  it("弹层 apply：光标在触发词之前时文本不错乱", async () => {
    const { dispose, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    const el = ta();
    el.focus();
    await userEvent.keyboard("帮我 /doc");
    await new Promise((r) => setTimeout(r, 400));
    expect(document.querySelector(".composer-popup")).not.toBeNull();
    // 旧实现 slice(0,start)+slice(cursor)：cursor 在触发词前会重复中段
    el.setSelectionRange(0, 0);
    el.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }),
    );
    expect(el.value).toBe("帮我 /doctor ");
    expect(document.querySelector(".composer-popup")).toBeNull();
    dispose();
  });

  it("textarea 失焦即关弹层", async () => {
    const { dispose, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    const el = ta();
    el.focus();
    await userEvent.keyboard("/doc");
    await new Promise((r) => setTimeout(r, 400));
    expect(document.querySelector(".composer-popup")).not.toBeNull();
    el.blur();
    expect(document.querySelector(".composer-popup")).toBeNull();
    dispose();
  });

  it("光标移出触发段即关弹层", async () => {
    const { dispose, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    const el = ta();
    el.focus();
    await userEvent.keyboard("/doc");
    await new Promise((r) => setTimeout(r, 400));
    expect(document.querySelector(".composer-popup")).not.toBeNull();
    // 合成 keydown 无原生光标位移，手动归位再补 keyup（真实 ArrowLeft/Home 的同态路径）
    el.setSelectionRange(0, 0);
    el.dispatchEvent(new KeyboardEvent("keyup", { key: "ArrowLeft", bubbles: true }));
    expect(document.querySelector(".composer-popup")).toBeNull();
    dispose();
  });

  it("点击弹层条目正常 apply（blur 不抢 click）", async () => {
    const { dispose, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    const el = ta();
    el.focus();
    await userEvent.keyboard("/doc");
    await new Promise((r) => setTimeout(r, 400));
    const btn = [...document.querySelectorAll<HTMLElement>(".composer-popup button")].find((b) =>
      b.textContent?.includes("/doctor"),
    )!;
    await userEvent.click(btn);
    expect(el.value).toBe("/doctor ");
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

  it("新会话首发：draft 旧键清空，下次新会话不恢复已发送内容", async () => {
    let sent = "";
    const { dispose, setTick, ta } = mount((t) => {
      sent = t;
      void ensureActiveSession();
    });
    await new Promise((r) => setTimeout(r, 100));
    setActiveSessionId("");
    setTick(1);
    await new Promise((r) => setTimeout(r, 50));
    ta().focus();
    await userEvent.keyboard("first message");
    await userEvent.keyboard("{Enter}");
    expect(sent).toBe("first message");
    // 落库完成：active id 变为真实会话，两个键都不留已发送内容
    await new Promise((r) => setTimeout(r, 200));
    expect(activeSessionId()).toBe("s-created");
    expect(getDraft("")).toBe("");
    expect(getDraft("s-created")).toBe("");
    // 下一次新会话：不得恢复已发送文本
    setActiveSessionId("");
    setTick(2);
    await new Promise((r) => setTimeout(r, 100));
    expect(ta().value).toBe("");
    dispose();
  });

  it("首发在途继续打字的草稿随落库迁移到新会话", async () => {
    let sent = "";
    chatMock.deferred = true;
    const { dispose, setTick, ta } = mount((t) => {
      sent = t;
      void ensureActiveSession();
    });
    await new Promise((r) => setTimeout(r, 100));
    setActiveSessionId("");
    setTick(1);
    await new Promise((r) => setTimeout(r, 50));
    ta().focus();
    await userEvent.keyboard("hello");
    await userEvent.keyboard("{Enter}");
    expect(sent).toBe("hello");
    expect(ta().value).toBe("");
    // 落库未完成时继续打字：先记在稳定键下
    await userEvent.keyboard(" wip");
    expect(getDraft("")).toBe(" wip");
    // 落库完成：草稿迁到真实会话并恢复，旧键清空
    for (const r of chatMock.resolvers.splice(0)) r(chatMock.meta());
    await new Promise((r) => setTimeout(r, 200));
    expect(activeSessionId()).toBe("s-created");
    expect(getDraft("")).toBe("");
    expect(getDraft("s-created")).toBe(" wip");
    expect(ta().value).toBe(" wip");
    dispose();
  });
});
