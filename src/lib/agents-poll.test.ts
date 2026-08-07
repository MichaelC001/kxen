// startAgentsPolling 实测：窗口 hidden 停表、回前台立即补一次、stop 后不再跑。
// browser 模式下不用 fake timers（路线不熟易 flaky）：10ms 短间隔真实等待。
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const refresh = vi.hoisted(() => ({ fn: vi.fn<() => Promise<void>>() }));
vi.mock("./state", () => ({ refreshAgents: refresh.fn }));

import { startAgentsPolling } from "./agents-poll";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

function setVisibility(v: "visible" | "hidden") {
  Object.defineProperty(document, "visibilityState", { configurable: true, value: v });
}

beforeEach(() => {
  refresh.fn.mockReset().mockResolvedValue(undefined);
  setVisibility("visible");
});

afterEach(() => {
  setVisibility("visible");
});

describe("startAgentsPolling", () => {
  it("visible 照跑；hidden 停表；回前台补一次；stop 后不再跑", async () => {
    const stop = startAgentsPolling(10);
    await sleep(35);
    const seenVisible = refresh.fn.mock.calls.length;
    expect(seenVisible).toBeGreaterThan(0);

    setVisibility("hidden");
    await sleep(35);
    expect(refresh.fn.mock.calls.length).toBe(seenVisible);

    setVisibility("visible");
    document.dispatchEvent(new Event("visibilitychange"));
    expect(refresh.fn.mock.calls.length).toBe(seenVisible + 1);

    stop();
    await sleep(35);
    expect(refresh.fn.mock.calls.length).toBe(seenVisible + 1);
  });
});
