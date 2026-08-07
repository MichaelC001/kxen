import { afterEach, describe, expect, it, vi } from "vitest";
import { relTime } from "./time";

afterEach(() => {
  vi.restoreAllMocks();
});

describe("relTime", () => {
  it("formats every relative-time range and old calendar dates", () => {
    const now = new Date(2026, 6, 27, 12, 0, 0).getTime();
    vi.spyOn(Date, "now").mockReturnValue(now);
    expect(relTime(now - 59_999)).toBe("刚刚");
    expect(relTime(now - 2 * 60_000)).toBe("2 分钟前");
    expect(relTime(now - 3 * 3_600_000)).toBe("3 小时前");
    expect(relTime(now - 86_400_000)).toBe("昨天");
    expect(relTime(now - 8 * 86_400_000)).toBe("8 天前");
    expect(relTime(new Date(2026, 4, 3).getTime())).toBe("5/3");
  });
});
