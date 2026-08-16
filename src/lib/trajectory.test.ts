// Trajectory 投影单测：call/result 合并行、unknown 字段处理、turn 分组、尾部分页、搜索、Overview 条。
import { describe, expect, it } from "vitest";
import type { StoredMessage } from "./chat";
import {
  filterTrajectory,
  groupTrajectoryTurns,
  overviewBars,
  recordsInRange,
  toTrajectoryRecords,
  toolDurationMs,
  trajectoryTailWindow,
  COMPACT_MARK,
  type TrajectoryRecord,
} from "./trajectory";

function msg(partial: Partial<StoredMessage> & Pick<StoredMessage, "id" | "role">): StoredMessage {
  return { session_id: "s1", parts: [], created_at: 0, ...partial };
}

describe("toTrajectoryRecords 记录类型", () => {
  it("user/assistant/system 文本各归其类，system 不再被跳过", () => {
    const records = toTrajectoryRecords([
      msg({ id: "m0", role: "system", parts: [{ type: "text", text: "系统提示" }] }),
      msg({ id: "m1", role: "user", parts: [{ type: "text", text: "帮我改 bug" }] }),
      msg({
        id: "m2",
        role: "assistant",
        parts: [{ type: "text", text: "好的" }],
        model: { provider: "p", model: "m" },
      }),
    ]);
    expect(records.map((r) => r.kind)).toEqual(["system", "user", "message"]);
    expect(records.map((r) => r.index)).toEqual([0, 1, 2]);
    expect(records[2]?.model).toEqual({ provider: "p", model: "m" });
  });

  it("tool_call 的 call+result 合并为一行，起止齐全给出耗时", () => {
    const records = toTrajectoryRecords([
      msg({
        id: "m1",
        role: "assistant",
        parts: [
          {
            type: "tool_call",
            name: "exec",
            input: "pnpm test",
            args: { command: "pnpm test" },
            output: "PASS",
            id: "call-1",
            started_at: 1000,
            finished_at: 1600,
          },
        ],
      }),
    ]);
    expect(records).toHaveLength(1);
    const tool = records[0]?.tool;
    expect(tool).toMatchObject({
      name: "exec",
      call: "pnpm test",
      result: "PASS",
      callId: "call-1",
    });
    expect(tool && toolDurationMs(tool)).toBe(600);
  });

  it("存量 JSONL 缺起止/args：字段缺省（unknown），绝不虚构", () => {
    const records = toTrajectoryRecords([
      msg({
        id: "m1",
        role: "assistant",
        parts: [{ type: "tool_call", name: "read", input: "a.ts", output: "x" }],
      }),
    ]);
    const tool = records[0]?.tool;
    expect(tool?.startedAt).toBeUndefined();
    expect(tool?.finishedAt).toBeUndefined();
    expect(tool?.args).toBeUndefined();
    expect(tool && toolDurationMs(tool)).toBeUndefined();
  });

  it("上下文注入（context / context_sources）出 context 记录并带来源归因", () => {
    const records = toTrajectoryRecords([
      msg({
        id: "m1",
        role: "user",
        parts: [
          { type: "context", text: "文件内容快照" },
          { type: "context_sources", items: [{ type: "file", path: "src/a.ts" }] },
        ],
      }),
    ]);
    expect(records.map((r) => r.kind)).toEqual(["context", "context"]);
    expect(records[1]?.source).toBe("file src/a.ts");
    expect(records[1]?.contextItems).toEqual([{ type: "file", path: "src/a.ts" }]);
  });

  it("压缩摘要消息（COMPACT_MARK 前缀）识别为 compacted 记录", () => {
    const records = toTrajectoryRecords([
      msg({
        id: "m1",
        role: "user",
        parts: [{ type: "text", text: `${COMPACT_MARK}\n前文摘要内容` }],
      }),
    ]);
    expect(records[0]?.kind).toBe("compacted");
    expect(records[0]?.text).toBe("前文摘要内容");
  });

  it("teammate 来源归因从落盘文本前缀读出，不硬编码名称表", () => {
    const records = toTrajectoryRecords([
      msg({ id: "m1", role: "user", parts: [{ type: "text", text: "[teammate builder] 已完成" }] }),
    ]);
    expect(records[0]?.source).toBe("teammate builder");
  });

  it("approval 落盘块按 tool 记录出（决策事件），reasoning 攒进同消息文本记录", () => {
    const records = toTrajectoryRecords([
      msg({
        id: "m1",
        role: "assistant",
        parts: [
          { type: "reasoning", text: "先想一下" },
          { type: "approval", command: "rm -rf x", reason: "危险", decision: "deny" },
          { type: "text", text: "已拒绝" },
        ],
      }),
    ]);
    expect(records.map((r) => r.kind)).toEqual(["tool", "message"]);
    expect(records[0]?.tool).toMatchObject({ name: "approval", call: "rm -rf x", result: "deny" });
    expect(records[1]?.reasoning).toBe("先想一下");
  });

  it("run 收尾消息的 stats 快照挂到 message 记录", () => {
    const records = toTrajectoryRecords([
      msg({
        id: "m1",
        role: "assistant",
        parts: [{ type: "text", text: "完成" }],
        stats: {
          ttft_ms: 300,
          duration_ms: 2000,
          input_tokens: 100,
          output_tokens: 50,
          tokens_per_sec: 25,
          usage_complete: true,
        },
        created_at: 10_000,
      }),
    ]);
    expect(records[0]?.stats?.duration_ms).toBe(2000);
  });
});

describe("groupTrajectoryTurns 边界与计数", () => {
  const records = toTrajectoryRecords([
    msg({ id: "m1", role: "user", parts: [{ type: "text", text: "第一问" }] }),
    msg({
      id: "m2",
      role: "assistant",
      parts: [
        { type: "tool_call", name: "read", input: "a", output: "x" },
        { type: "tool_call", name: "exec", input: "b", output: "y" },
        { type: "text", text: "答一" },
      ],
    }),
    msg({ id: "m3", role: "user", parts: [{ type: "text", text: "第二问" }] }),
    msg({ id: "m4", role: "assistant", parts: [{ type: "text", text: "答二" }] }),
  ]);

  it("user 记录开启新 turn，步骤与工具调用计数正确", () => {
    const turns = groupTrajectoryTurns(records);
    expect(turns).toHaveLength(2);
    expect(turns[0]).toMatchObject({ steps: 4, toolCalls: 2, headed: true });
    expect(turns[1]).toMatchObject({ steps: 2, toolCalls: 0, headed: true });
  });

  it("首个 user 之前的记录归入序幕 turn（headed=false）", () => {
    const turns = groupTrajectoryTurns(records.slice(1));
    expect(turns[0]?.headed).toBe(false);
    expect(turns[0]?.steps).toBe(3);
  });
});

describe("trajectoryTailWindow 尾部优先分页", () => {
  const all = Array.from({ length: 250 }, (_, i) => i);

  it("默认定位最新尾部，hasEarlier 标记还有更早页", () => {
    const { window, hasEarlier } = trajectoryTailWindow(all, 100);
    expect(window[0]).toBe(150);
    expect(window.at(-1)).toBe(249);
    expect(hasEarlier).toBe(true);
  });

  it("不足一页时全量返回且无更早页；逐步扩窗直至覆盖全表", () => {
    expect(trajectoryTailWindow([1, 2], 100)).toEqual({ window: [1, 2], hasEarlier: false });
    const { window, hasEarlier } = trajectoryTailWindow(all, 300);
    expect(window).toHaveLength(250);
    expect(hasEarlier).toBe(false);
  });
});

describe("filterTrajectory 搜索", () => {
  const records = toTrajectoryRecords([
    msg({ id: "m1", role: "user", parts: [{ type: "text", text: "修复登录 Bug" }] }),
    msg({
      id: "m2",
      role: "assistant",
      parts: [{ type: "tool_call", name: "grep", input: "authToken", output: "src/auth.ts" }],
    }),
  ]);

  it("覆盖摘要/工具名/参数/结果，大小写不敏感；空串返回全表", () => {
    expect(filterTrajectory(records, "authtoken")).toHaveLength(1);
    expect(filterTrajectory(records, "登录")).toHaveLength(1);
    expect(filterTrajectory(records, "src/auth.ts")).toHaveLength(1);
    expect(filterTrajectory(records, "")).toHaveLength(2);
    expect(filterTrajectory(records, "不存在")).toHaveLength(0);
  });
});

describe("overviewBars 计时投影", () => {
  it("message 条按 stats 回推起点并带 TTFT；tool 条用实测起止；无计时不出条", () => {
    const records: TrajectoryRecord[] = toTrajectoryRecords([
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
            started_at: 5000,
            finished_at: 5300,
          },
          { type: "tool_call", name: "read", input: "a", output: "b" },
          { type: "text", text: "无 stats 的消息" },
        ],
      }),
    ]);
    const bars = overviewBars(records);
    expect(bars).toHaveLength(2);
    expect(bars[0]).toMatchObject({ kind: "message", start: 8000, end: 10_000, ttftMs: 400 });
    expect(bars[1]).toMatchObject({ kind: "tool", start: 5000, end: 5300, label: "exec" });
  });

  it("拖选闭区间：有条记录按条重叠，无条记录按 time 落点", () => {
    const records: TrajectoryRecord[] = toTrajectoryRecords([
      msg({
        id: "m1",
        role: "assistant",
        created_at: 10_000,
        parts: [{ type: "text", text: "完成" }],
        stats: {
          ttft_ms: 100,
          duration_ms: 2000,
          input_tokens: 1,
          output_tokens: 1,
          tokens_per_sec: 1,
        },
      }),
      msg({ id: "m2", role: "user", created_at: 9000, parts: [{ type: "text", text: "问" }] }),
      msg({
        id: "m3",
        role: "user",
        created_at: 20_000,
        parts: [{ type: "text", text: "区间外" }],
      }),
    ]);
    const bars = overviewBars(records);
    // 闭区间 [8500, 9000]：m1 条 [8000,10000] 重叠命中；m2 time=9000 落在闭区间命中；m3 不命中
    const hit = recordsInRange(records, bars, 8500, 9000);
    expect([...hit].sort()).toEqual([0, 1]);
  });
});
