// rewind 确认流：dirty 门禁的"拒绝 -> 确认 -> 带 confirm 重发"序列，其余拒绝不重试。
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  classifyRewindError,
  createRewindFlow,
  createSessionRewind,
  rewindErrorText,
} from "./rewind";

// 与 src-tauri/src/ws/session_ops.rs 的门禁文案逐字对齐，漂移即测试红
const DIRTY = new Error("worktree has uncheckpointed changes, pass confirm=true to rewind anyway");
const ACTIVE_RUN = new Error("workspace has an active run, rewind refused");
const NOT_IN_SESSION = new Error("message not found: m-1");

type Call = { sid: string; mid: string; confirm: boolean };

function harness(rejectOnce?: Error, rejectAlways?: Error) {
  const calls: Call[] = [];
  const done: string[] = [];
  const errors: string[] = [];
  const pendings: (string | null)[] = [];
  const flow = createRewindFlow({
    sessionId: () => "s1",
    call: (sid, mid, confirm) => {
      calls.push({ sid, mid, confirm });
      const err = rejectAlways ?? (calls.length === 1 ? rejectOnce : undefined);
      return err ? Promise.reject(err) : Promise.resolve();
    },
    onPendingChange: (id) => pendings.push(id),
    onDone: () => done.push("done"),
    onError: (t) => errors.push(t),
  });
  return { flow, calls, done, errors, pendings };
}

describe("createRewindFlow", () => {
  it("dirty 拒绝后进待确认，用户确认带 confirm=true 重发同一消息", async () => {
    const h = harness(DIRTY);
    await h.flow.request("m-9");
    // 第一次不带 confirm，被拒后挂起等待用户决定，不算完成也不算错误
    expect(h.calls).toEqual([{ sid: "s1", mid: "m-9", confirm: false }]);
    expect(h.flow.pending()).toBe("m-9");
    expect(h.done).toEqual([]);
    expect(h.errors).toEqual([]);

    await h.flow.confirm();
    expect(h.calls).toEqual([
      { sid: "s1", mid: "m-9", confirm: false },
      { sid: "s1", mid: "m-9", confirm: true },
    ]);
    expect(h.flow.pending()).toBeNull();
    expect(h.done).toEqual(["done"]);
    expect(h.errors).toEqual([]);
  });

  it("active run 拒绝：直接报错，不重试、不进待确认、confirm 空转", async () => {
    const h = harness(ACTIVE_RUN, ACTIVE_RUN);
    await h.flow.request("m-1");
    expect(h.calls).toEqual([{ sid: "s1", mid: "m-1", confirm: false }]);
    expect(h.flow.pending()).toBeNull();
    expect(h.done).toEqual([]);
    expect(h.errors).toEqual([
      "工作区有任务正在运行，回退会覆盖它正在写的文件，请先停止或等它完成",
    ]);

    // 无待确认项时 confirm 不得触发任何重发
    await h.flow.confirm();
    expect(h.calls).toHaveLength(1);
  });

  it("跨 session 拒绝：不重试，文案指向消息归属", async () => {
    const h = harness(NOT_IN_SESSION, NOT_IN_SESSION);
    await h.flow.request("m-1");
    expect(h.calls).toHaveLength(1);
    expect(h.flow.pending()).toBeNull();
    expect(h.errors).toEqual(["这条消息不在当前会话中，无法回退到此处"]);
  });

  it("取消待确认：清空 pending 且不再重发", async () => {
    const h = harness(DIRTY);
    await h.flow.request("m-9");
    expect(h.flow.pending()).toBe("m-9");
    h.flow.cancel();
    expect(h.flow.pending()).toBeNull();
    await h.flow.confirm();
    expect(h.calls).toHaveLength(1);
    expect(h.done).toEqual([]);
  });

  it("confirm 重发仍被拒（确认期间起了 run）：走错误提示，不再挂确认", async () => {
    const calls: Call[] = [];
    const errors: string[] = [];
    const flow = createRewindFlow({
      sessionId: () => "s1",
      call: (sid, mid, confirm) => {
        calls.push({ sid, mid, confirm });
        return Promise.reject(confirm ? ACTIVE_RUN : DIRTY);
      },
      onError: (t) => errors.push(t),
    });
    await flow.request("m-9");
    expect(flow.pending()).toBe("m-9");
    await flow.confirm();
    expect(flow.pending()).toBeNull();
    expect(calls.map((c) => c.confirm)).toEqual([false, true]);
    expect(errors).toEqual(["工作区有任务正在运行，回退会覆盖它正在写的文件，请先停止或等它完成"]);
  });
});

describe("classifyRewindError / rewindErrorText", () => {
  it("按后端文案子串归类三种门禁", () => {
    expect(classifyRewindError(DIRTY)).toBe("dirty");
    expect(classifyRewindError(ACTIVE_RUN)).toBe("active_run");
    expect(classifyRewindError(NOT_IN_SESSION)).toBe("not_in_session");
    expect(classifyRewindError(new Error("rpc timeout: session.rewind"))).toBe("unknown");
  });

  it("未识别错误保留原始信息便于排查", () => {
    expect(rewindErrorText(new Error("boom"))).toBe("回退失败：boom");
  });
});

describe("createSessionRewind 错误尾注", () => {
  afterEach(() => vi.useRealTimers());

  function noteHarness() {
    vi.useFakeTimers();
    const r = createSessionRewind({
      sessionId: () => "s1",
      onDone: () => {},
      call: () => Promise.reject(ACTIVE_RUN),
    });
    return { note: r.note, dismiss: r.dismissNote, fire: (mid: string) => r.flow.request(mid) };
  }

  it("报错上尾注，4s 自动消失", async () => {
    const h = noteHarness();
    await h.fire("m-1");
    expect(h.note()).toContain("正在运行");
    vi.advanceTimersByTime(3999);
    expect(h.note()).not.toBe("");
    vi.advanceTimersByTime(1);
    expect(h.note()).toBe("");
  });

  it("点击关闭立即消，且旧计时器不再清掉后续文案", async () => {
    const h = noteHarness();
    await h.fire("m-1");
    expect(h.note()).not.toBe("");
    h.dismiss();
    expect(h.note()).toBe("");
    // 再次报错：新文案不被第一次的计时器抢清
    await h.fire("m-2");
    vi.advanceTimersByTime(3999);
    expect(h.note()).not.toBe("");
    vi.advanceTimersByTime(1);
    expect(h.note()).toBe("");
  });
});
