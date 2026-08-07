import { beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  startDragging: vi.fn(),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ startDragging: h.startDragging }),
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
});
