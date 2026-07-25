// UsageSection 回归：usage.overview RPC 失败显错误态（带重试），不把加载失败渲成全零；
// 真零（成功返回空数据）才显示 0 与「暂无派发记录」。
import { render } from "solid-js/web";
import { afterEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  rpc: vi.fn((_method: string, _params?: unknown) => Promise.resolve({}) as Promise<unknown>),
}));

vi.mock("../../lib/client", () => ({ client: { rpc: h.rpc } }));

import UsageSection from "./UsageSection";

const EMPTY = { total_input: 0, total_output: 0, sessions: 0, dispatches: 0, by_model: {} };

afterEach(() => {
  document.body.innerHTML = "";
  vi.clearAllMocks();
});

describe("UsageSection 加载失败与真零区分", () => {
  it("RPC 失败：错误态 + 重试，不显示全零假象", async () => {
    h.rpc.mockRejectedValue(new Error("ws closed"));
    const dispose = render(() => <UsageSection />, document.body);
    await vi.waitFor(() =>
      expect(document.body.textContent).toContain("加载用量统计失败：ws closed"),
    );
    expect(document.body.textContent).not.toContain("暂无派发记录");

    // 重试成功：错误态消失，真零正常显示
    h.rpc.mockResolvedValue(EMPTY);
    const retry = [...document.body.querySelectorAll<HTMLButtonElement>("button")].find(
      (b) => b.textContent === "重试",
    );
    if (!retry) throw new Error("retry button not found");
    retry.click();
    await vi.waitFor(() => expect(document.body.textContent).toContain("暂无派发记录"));
    expect(document.body.textContent).not.toContain("加载用量统计失败");
    dispose();
  });

  it("成功返回真零：显示 0 与空分布，无错误态", async () => {
    h.rpc.mockResolvedValue(EMPTY);
    const dispose = render(() => <UsageSection />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("暂无派发记录"));
    expect(document.body.textContent).not.toContain("加载用量统计失败");
    dispose();
  });
});
