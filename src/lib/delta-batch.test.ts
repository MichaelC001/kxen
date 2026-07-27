import { afterEach, describe, expect, it, vi } from "vitest";
import { createDeltaBatcher } from "./delta-batch";

afterEach(() => {
  vi.useRealTimers();
});

describe("createDeltaBatcher", () => {
  it("coalesces both fields into one scheduled flush", () => {
    vi.useFakeTimers();
    const append = vi.fn();
    const batcher = createDeltaBatcher(append, 50);

    batcher.push("content", "a");
    batcher.push("content", "b");
    batcher.push("reasoning", "r");
    expect(append).not.toHaveBeenCalled();

    vi.advanceTimersByTime(50);
    expect(append.mock.calls).toEqual([
      ["content", "ab"],
      ["reasoning", "r"],
    ]);
  });

  it("flushNow drains pending content and is inert when empty", () => {
    vi.useFakeTimers();
    const append = vi.fn();
    const batcher = createDeltaBatcher(append);

    batcher.flushNow();
    batcher.push("reasoning", "thought");
    batcher.flushNow();
    batcher.flushNow();

    expect(append).toHaveBeenCalledOnce();
    expect(append).toHaveBeenCalledWith("reasoning", "thought");
  });
});
