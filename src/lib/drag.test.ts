import { beforeEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  startDragging: vi.fn(),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ startDragging: h.startDragging }),
}));

import { onDragStart } from "./drag";

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
});
