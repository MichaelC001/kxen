// ContextGauge：圆环占用指示 + 组成明细弹层。估算值一律带 ~，实测锚点不带；RPC 失败显示错误行。
import { render } from "solid-js/web";
import { createSignal } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ContextStats } from "../../lib/context-stats";

const statsMock = vi.hoisted(() => ({
  result: null as ContextStats | null,
  error: null as Error | null,
}));
vi.mock("../../lib/context-stats", () => ({
  sessionContextStats: async () => {
    if (statsMock.error) throw statsMock.error;
    return statsMock.result;
  },
}));

import ContextGauge from "./ContextGauge";

const fixture: ContextStats = {
  system_tokens: 1000,
  tool_tokens: 500,
  message_tokens: 500,
  window_tokens: 10000,
  last_input_tokens: 3210,
};

function mount(stats: ContextStats | null = fixture) {
  statsMock.result = stats;
  statsMock.error = null;
  const [sessionId] = createSignal("s1");
  const [streaming] = createSignal(false);
  const dispose = render(
    () => <ContextGauge sessionId={sessionId} streaming={streaming} />,
    document.body,
  );
  return { dispose };
}

const gauge = () =>
  document.body.querySelector<HTMLButtonElement>("[data-testid='context-gauge']")!;
const openDetail = () => gauge().dispatchEvent(new MouseEvent("click", { bubbles: true }));

afterEach(() => {
  document.body.innerHTML = "";
  statsMock.result = null;
  statsMock.error = null;
});

describe("ContextGauge", () => {
  it("圆环 + ~pct%：估算合计 / 窗口，占比取整", async () => {
    const { dispose } = mount();
    await vi.waitFor(() => expect(gauge().textContent).toContain("~20%"));
    expect(gauge().querySelector("svg")).toBeTruthy();
    dispose();
  });

  it("点击展开明细：三段拆分带 ~ 前缀，合计/窗口与实测锚点（精确）并列", async () => {
    const { dispose } = mount();
    await vi.waitFor(() => expect(gauge().textContent).toContain("~20%"));
    openDetail();
    const detail = document.body.querySelector("[data-testid='context-gauge-detail']")!;
    expect(detail).toBeTruthy();
    await vi.waitFor(() => expect(detail.textContent).toContain("系统提示词"));
    expect(detail.textContent).toContain("~1000 tok");
    expect(detail.textContent).toContain("~500 tok");
    expect(detail.textContent).toContain("~2000 / 10000 tok（~20%）");
    expect(detail.textContent).toContain("3210 tok（精确）");
    dispose();
  });

  it("尚无实测输入（last_input_tokens null）显示未知，不编造", async () => {
    const { dispose } = mount({ ...fixture, last_input_tokens: null });
    await vi.waitFor(() => expect(gauge().textContent).toContain("~20%"));
    openDetail();
    const detail = document.body.querySelector("[data-testid='context-gauge-detail']")!;
    await vi.waitFor(() => expect(detail.textContent).toContain("未知"));
    expect(detail.textContent).not.toContain("精确");
    dispose();
  });

  it("RPC 失败：明细弹层显示错误行", async () => {
    statsMock.error = new Error("session not found");
    const [sessionId] = createSignal("s1");
    const [streaming] = createSignal(false);
    const dispose = render(
      () => <ContextGauge sessionId={sessionId} streaming={streaming} />,
      document.body,
    );
    await vi.waitFor(() => expect(gauge()).toBeTruthy());
    openDetail();
    await vi.waitFor(() =>
      expect(document.body.textContent).toContain("组成明细加载失败：session not found"),
    );
    dispose();
  });

  it("无会话（sessionId 空）整体隐藏", () => {
    const [sessionId] = createSignal("");
    const [streaming] = createSignal(false);
    const dispose = render(
      () => <ContextGauge sessionId={sessionId} streaming={streaming} />,
      document.body,
    );
    expect(document.body.querySelector("[data-testid='context-gauge']")).toBeNull();
    dispose();
  });
});
