import { beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  startDragging: vi.fn(),
  // 裸函数桩：vi.fn 会给返回的 promise 挂 settled 追踪 handler，探测不到 onDragStart 自己的 catch
  impl: undefined as (() => Promise<void>) | undefined,
  handled: false,
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    startDragging: () => (h.impl ? h.impl() : h.startDragging()),
  }),
}));

import { onDragStart } from "./drag";

const w = window as unknown as { __TAURI_INTERNALS__?: unknown };

function event(button: number, target: HTMLElement): MouseEvent {
  const value = new MouseEvent("mousedown", { button });
  Object.defineProperty(value, "target", { value: target });
  return value;
}

beforeEach(() => {
  h.startDragging.mockReset();
  h.startDragging.mockResolvedValue(undefined);
  h.impl = undefined;
  h.handled = false;
});

describe("onDragStart", () => {
  it("starts dragging from a left-button blank region", () => {
    onDragStart(event(0, document.createElement("div")));
    expect(h.startDragging).toHaveBeenCalledOnce();
  });

  it("ignores non-left clicks and interactive descendants", () => {
    onDragStart(event(1, document.createElement("div")));
    const button = document.createElement("button");
    const span = document.createElement("span");
    button.append(span);
    onDragStart(event(0, span));
    expect(h.startDragging).not.toHaveBeenCalled();
  });

  it("web 模式（无 __TAURI_INTERNALS__）不调用窗口 API：浏览器里 startDragging 会抛", () => {
    const saved = w.__TAURI_INTERNALS__;
    delete w.__TAURI_INTERNALS__;
    try {
      onDragStart(event(0, document.createElement("div")));
      expect(h.startDragging).not.toHaveBeenCalled();
    } finally {
      w.__TAURI_INTERNALS__ = saved;
    }
  });

  it("startDragging 的 IPC rejection 被消化（不成 unhandled rejection）", () => {
    let floating: Promise<void> | undefined;
    h.impl = () => {
      const p = Promise.reject(new Error("ipc down")) as Promise<void>;
      const then = p.then.bind(p);
      // oxlint-disable-next-line unicorn/no-thenable -- 测试插桩：探测 onDragStart 是否挂了 rejection handler
      p.then = (onFulfilled, onRejected) => {
        if (onRejected) h.handled = true;
        return then(onFulfilled, onRejected);
      };
      floating = p;
      return p;
    };
    onDragStart(event(0, document.createElement("div")));
    expect(h.handled).toBe(true);
    // 收尾挂 noop handler：避免测试自身遗留 unhandled rejection 干扰其他用例
    void floating?.catch(() => {});
  });
});
