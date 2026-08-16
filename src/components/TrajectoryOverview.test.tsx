// Overview 时间线实测：计时投影渲染、左键拖选聚焦、右键单击清除、悬停 tooltip、无数据降级。
import { render } from "solid-js/web";
import { createSignal } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { StoredMessage } from "../lib/chat";
import { toTrajectoryRecords, type TrajectoryRecord } from "../lib/trajectory";
import TrajectoryOverview, { type TimeRange } from "./TrajectoryOverview";

function msg(partial: Partial<StoredMessage> & Pick<StoredMessage, "id" | "role">): StoredMessage {
  return { session_id: "s1", parts: [], created_at: 0, ...partial };
}

function timedRecords(): TrajectoryRecord[] {
  return toTrajectoryRecords([
    msg({
      id: "m1",
      role: "assistant",
      created_at: 10_000,
      parts: [{ type: "text", text: "完成" }],
      stats: {
        ttft_ms: 400,
        duration_ms: 2000,
        input_tokens: 1,
        output_tokens: 1,
        tokens_per_sec: 1,
        usage_complete: true,
      },
    }),
    msg({
      id: "m2",
      role: "assistant",
      parts: [
        {
          type: "tool_call",
          name: "exec",
          input: "ls",
          output: "ok",
          started_at: 4000,
          finished_at: 4500,
        },
      ],
    }),
  ]);
}

function mount(records: TrajectoryRecord[]) {
  const [selection, setSelection] = createSignal<TimeRange | undefined>();
  const dispose = render(
    () => (
      <TrajectoryOverview records={() => records} selection={selection} onSelect={setSelection} />
    ),
    document.body,
  );
  return { dispose, selection };
}

const track = () => document.body.querySelector(".cursor-crosshair")!;
const pointer = (type: string, options: PointerEventInit) =>
  track().dispatchEvent(new PointerEvent(type, { bubbles: true, ...options }));

afterEach(() => {
  document.body.innerHTML = "";
});

describe("TrajectoryOverview", () => {
  it("已加载记录按真实起止投影为时间条（message + tool 各一条）", () => {
    const { dispose } = mount(timedRecords());
    expect(document.body.querySelectorAll("[data-bar]").length).toBe(2);
    expect(document.body.textContent).toContain("6.0s"); // 域宽标签（4000..10000ms）
    dispose();
  });

  it("无计时数据的记录不投影，显示降级提示", () => {
    const { dispose } = mount(
      toTrajectoryRecords([msg({ id: "m1", role: "user", parts: [{ type: "text", text: "问" }] })]),
    );
    expect(document.body.querySelectorAll("[data-bar]").length).toBe(0);
    expect(document.body.textContent).toContain("无计时数据");
    dispose();
  });

  it("左键拖选产生选区并回调，右键单击清除选区", () => {
    const { dispose, selection } = mount(timedRecords());
    const rect = track().getBoundingClientRect();
    pointer("pointerdown", { button: 0, clientX: rect.left + 10 });
    pointer("pointermove", { button: 0, clientX: rect.left + rect.width - 10 });
    pointer("pointerup", { button: 0, clientX: rect.left + rect.width - 10 });
    expect(selection()).toBeDefined();
    expect(document.body.querySelector("[data-testid='overview-selection']")).toBeTruthy();
    // 右键单击（未拖动）清除
    pointer("pointerdown", { button: 2, clientX: rect.left + 50 });
    pointer("pointerup", { button: 2, clientX: rect.left + 50 });
    expect(selection()).toBeUndefined();
    dispose();
  });

  it("右键拖动平移时间域：域标签随之改变", () => {
    const { dispose } = mount(timedRecords());
    const before = document.body.textContent ?? "";
    const rect = track().getBoundingClientRect();
    pointer("pointerdown", { button: 2, clientX: rect.left + 100 });
    pointer("pointermove", { button: 2, clientX: rect.left + 20 });
    pointer("pointerup", { button: 2, clientX: rect.left + 20 });
    expect(document.body.textContent).not.toBe(before);
    dispose();
  });

  it("悬停 500ms 显示精确起止/总耗时/TTFT/解码 tooltip", async () => {
    const { dispose } = mount(timedRecords());
    const bar = document.body.querySelector("[data-bar='0']")!;
    bar.dispatchEvent(new PointerEvent("pointermove", { bubbles: true, clientX: 5, clientY: 5 }));
    expect(document.body.querySelector("[data-testid='overview-tooltip']")).toBeNull();
    await vi.waitFor(
      () => expect(document.body.querySelector("[data-testid='overview-tooltip']")).toBeTruthy(),
      {
        timeout: 1500,
      },
    );
    const text = document.body.querySelector("[data-testid='overview-tooltip']")!.textContent ?? "";
    expect(text).toContain("总耗时 2.0s");
    expect(text).toContain("TTFT 400ms");
    expect(text).toContain("解码 1.6s");
    dispose();
  });
});
