// agent-display 收口函数实测：已知值走映射，未知值回显原文/灰点不渲染空白。
import { describe, expect, it } from "vitest";
import { kindBadge, statusText, statusTone } from "./agent-display";

describe("agent-display", () => {
  it("已知状态/kind 走映射", () => {
    expect(statusText("working")).toBe("工作中");
    expect(statusTone("failed")).toEqual({ tone: "err", pulse: false });
    expect(kindBadge("teammate")).toBe("team");
  });

  it("未知值回显原文/灰点，不渲染空白", () => {
    expect(statusText("hibernating")).toBe("hibernating");
    expect(statusTone("hibernating")).toEqual({ tone: "faint", pulse: false });
    expect(kindBadge("cron")).toBe("cron");
  });
});
