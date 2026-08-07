// TextComposer 弹层/触发实测（350 行门禁从 text-composer.test.tsx 拆出）：
// slash 行首契约、@/# 全角触发、IME 弹层守卫、apply 定界、失焦/移出关闭、ARIA 与锚点跟随。
import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import "../../styles.css";
import { afterEach, describe, expect, it, vi } from "vitest";
import { userEvent } from "@vitest/browser/context";
import TextComposer from "./TextComposer";
import { setActiveSessionId } from "../../lib/state";
import { clearDraft } from "../../lib/drafts";

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

describe("TextComposer 弹层/触发 (webkit)", () => {
  it("slash 中段不触发弹层（后端只展开消息开头命令，空白前界也不行）", async () => {
    const { dispose, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    ta().focus();
    await userEvent.keyboard("帮我 /doc");
    await new Promise((r) => setTimeout(r, 400));
    expect(document.querySelector(".composer-popup")).toBeNull();
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
    // 弹层必须落在视口内（镜像定位曾经量到页面原点，弹出屏幕外）
    const rect = popup!.getBoundingClientRect();
    expect(rect.bottom).toBeGreaterThan(0);
    expect(rect.top).toBeLessThan(window.innerHeight);
    dispose();
  });

  it("全角括号后的 / 不触发（非行首）", async () => {
    const { dispose, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    ta().focus();
    await userEvent.keyboard("（/doc");
    await new Promise((r) => setTimeout(r, 400));
    expect(document.querySelector(".composer-popup")).toBeNull();
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
    await userEvent.keyboard("帮我{Shift>}{Enter}{/Shift}/doc");
    await new Promise((r) => setTimeout(r, 400));
    expect(document.querySelector(".composer-popup")).not.toBeNull();
    // 旧实现 slice(0,start)+slice(cursor)：cursor 在触发词前会重复中段
    el.setSelectionRange(0, 0);
    el.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }),
    );
    expect(el.value).toBe("帮我\n/doctor ");
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

  it("弹层 ARIA 齐备：listbox/option/aria-selected/activedescendant", async () => {
    const { dispose, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    ta().focus();
    await userEvent.keyboard("/d");
    await new Promise((r) => setTimeout(r, 400));
    const listbox = document.querySelector(".composer-popup")!;
    expect(listbox.getAttribute("role")).toBe("listbox");
    const opts = [...document.querySelectorAll<HTMLElement>("[role=option]")];
    expect(opts.length).toBeGreaterThanOrEqual(2);
    // 选中态契约：恰好一项 aria-selected=true 且与 activedescendant 同指
    // （不断言固定选中首项：上个用例的鼠标停在弹层位置，mouseenter 合一选中是设计行为）
    const sel = opts.filter((o) => o.getAttribute("aria-selected") === "true");
    expect(sel.length).toBe(1);
    expect(listbox.getAttribute("aria-activedescendant")).toBe(sel[0]!.id);
    dispose();
  });

  it("hover 与键盘选中合一 + 选中项滚动跟随", async () => {
    const scrollSpy = vi.spyOn(Element.prototype, "scrollIntoView").mockImplementation(() => {});
    const { dispose, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    const el = ta();
    el.focus();
    await userEvent.keyboard("/d");
    await new Promise((r) => setTimeout(r, 400));
    const opts = [...document.querySelectorAll<HTMLElement>("[role=option]")];
    expect(opts.length).toBeGreaterThanOrEqual(2);
    // hover 第 2 条：选中态合并（不再 hover/键盘双高亮）
    opts[1]!.dispatchEvent(new MouseEvent("mouseenter", { bubbles: true }));
    await new Promise((r) => setTimeout(r, 30));
    expect(opts[1]!.getAttribute("aria-selected")).toBe("true");
    expect(opts[1]!.classList.contains("bg-[var(--bg-overlay)]")).toBe(true);
    expect(opts[0]!.getAttribute("aria-selected")).toBe("false");
    // 键盘导航：选中项 scrollIntoView({block:"nearest"}) 跟随
    scrollSpy.mockClear();
    el.dispatchEvent(
      new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true, cancelable: true }),
    );
    await new Promise((r) => setTimeout(r, 30));
    expect(scrollSpy).toHaveBeenCalled();
    scrollSpy.mockRestore();
    dispose();
  });

  it("弹层锚点随输入即算不冻结（query 变长光标右移，left 跟随）", async () => {
    const { dispose, ta } = mount();
    await new Promise((r) => setTimeout(r, 100));
    ta().focus();
    await userEvent.keyboard("/d");
    await new Promise((r) => setTimeout(r, 400));
    const popup = () => document.querySelector<HTMLElement>(".composer-popup")!;
    const left1 = parseFloat(popup().style.left);
    await userEvent.keyboard("o");
    await new Promise((r) => setTimeout(r, 50));
    const left2 = parseFloat(popup().style.left);
    expect(left2).toBeGreaterThan(left1);
    dispose();
  });
});
