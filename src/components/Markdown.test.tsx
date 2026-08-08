// Markdown mermaid 降级：renderMermaid 失败（库加载失败等）时组件必须消化 rejection
// （否则成 unhandled rejection），占位 div 保留原文源码，界面不空白。
// 不用 vi.fn 桩 renderMermaid：vitest 4 mock 会给返回的 promise 挂 settled 追踪 handler，
// 会污染「组件是否消化 rejection」的探测。
import { render } from "solid-js/web";
import { afterEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  calls: 0,
  handled: false,
  failNext: false,
  lastPromise: undefined as Promise<void> | undefined,
}));

vi.mock("../lib/markdown", () => ({
  renderMarkdown: async (text: string) => `<div class="mermaid">${text}</div>`,
  renderMermaid: (_el: HTMLElement): Promise<void> => {
    h.calls += 1;
    if (!h.failNext) return Promise.resolve();
    // 探测组件是否给返回的 promise 挂 rejection handler（WebKit 的 unhandledrejection
    // 派发依赖 GC 时机，不能靠事件断言）
    const p = Promise.reject(new Error("mermaid load failed"));
    const then = p.then.bind(p);
    // oxlint-disable-next-line unicorn/no-thenable -- 测试插桩：探测组件是否挂了 rejection handler
    p.then = (onFulfilled, onRejected) => {
      if (onRejected) h.handled = true;
      return then(onFulfilled, onRejected);
    };
    h.lastPromise = p;
    return p;
  },
}));

import Markdown from "./Markdown";

afterEach(() => {
  document.body.innerHTML = "";
  h.calls = 0;
  h.handled = false;
  h.failNext = false;
  h.lastPromise = undefined;
});

describe("Markdown mermaid 降级", () => {
  it("renderMermaid 失败时组件消化 rejection，占位原文保留", async () => {
    h.failNext = true;
    const dispose = render(() => <Markdown text="graph TD; A-->B;" />, document.body);
    await vi.waitFor(() => expect(h.calls).toBe(1));
    expect(h.handled).toBe(true);
    expect(document.body.querySelector(".mermaid")?.textContent).toBe("graph TD; A-->B;");
    dispose();
    // 收尾挂 noop handler：避免测试自身遗留 unhandled rejection 干扰其他用例
    await h.lastPromise?.catch(() => {});
  });
});
