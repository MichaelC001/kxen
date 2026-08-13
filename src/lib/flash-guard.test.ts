// flash / async-guard 单测：timer 用假时钟（vitest fake timers）。
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createFlash } from "./flash";
import {
  createAction,
  createInFlight,
  createReconciledMutation,
  createSeqGuard,
} from "./async-guard";

describe("flash", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("show 后按时消失，连续消息不抢清", () => {
    const f = createFlash(1000);
    f.show("a");
    vi.advanceTimersByTime(500);
    f.show("b");
    vi.advanceTimersByTime(500);
    expect(f.msgs().map((m) => m.text)).toEqual(["b"]); // a 到点消失，b 还活着
    vi.advanceTimersByTime(500);
    expect(f.msgs()).toEqual([]);
  });

  it("dismiss 立即移除并停表", () => {
    const f = createFlash(1000);
    f.show("a");
    const id = f.msgs()[0]!.id;
    f.dismiss(id);
    vi.advanceTimersByTime(2000);
    expect(f.msgs()).toEqual([]);
  });

  it("最多堆 3 条", () => {
    const f = createFlash(0);
    for (const t of ["a", "b", "c", "d", "e"]) f.show(t);
    expect(f.msgs().map((m) => m.text)).toEqual(["c", "d", "e"]);
  });
});

describe("async-guard", () => {
  it("seq 只有最后一次有效", () => {
    const g = createSeqGuard();
    const a = g.next();
    const b = g.next();
    expect(g.isCurrent(a)).toBe(false);
    expect(g.isCurrent(b)).toBe(true);
  });

  it("in-flight 同 key 共享 promise", async () => {
    const dedupe = createInFlight();
    let calls = 0;
    const fn = async () => {
      calls++;
      await new Promise((r) => setTimeout(r, 10));
      return 1;
    };
    const [x, y] = await Promise.all([dedupe("k", fn), dedupe("k", fn)]);
    expect([x, y]).toEqual([1, 1]);
    expect(calls).toBe(1);
    // finally 清理后下一批重新执行
    await dedupe("k", fn);
    expect(calls).toBe(2);
  });

  it("action 三态：pending 拒连点、失败走 flashErr 且不 throw", async () => {
    const a = createAction();
    let done = 0;
    const slow = async () => {
      await new Promise((r) => setTimeout(r, 20));
      done++;
    };
    const p1 = a.run(slow);
    const p2 = a.run(slow); // pending 中被拒
    await Promise.all([p1, p2]);
    expect(done).toBe(1);
    const r = await a.run(async () => {
      throw new Error("boom");
    });
    expect(r).toBeUndefined(); // 失败不外抛
  });

  it("reconciled mutation 在失败重试时复用业务 ID 和 idempotency token", async () => {
    let prepared = 0;
    let attempts = 0;
    let changed = 0;
    const seen: Array<{ resourceId: string; idempotencyKey: string }> = [];
    const action = createReconciledMutation({
      refresh: async () => undefined,
      onChanged: () => changed++,
    });
    const mutation = {
      key: "create:report",
      prepare: () => {
        prepared++;
        return { resourceId: "bot_stable", idempotencyKey: "idem_stable" };
      },
      execute: async (token: { resourceId: string; idempotencyKey: string }) => {
        seen.push(token);
        attempts++;
        if (attempts === 1) throw new Error("transport timeout");
      },
      okText: "created",
    };

    expect(await action.run(mutation)).toBe("failed");
    expect(action.hasRetry(mutation.key)).toBe(true);
    expect(await action.run(mutation)).toBe("applied");
    expect(prepared).toBe(1);
    expect(seen).toEqual([
      { resourceId: "bot_stable", idempotencyKey: "idem_stable" },
      { resourceId: "bot_stable", idempotencyKey: "idem_stable" },
    ]);
    expect(changed).toBe(1);
    expect(action.hasRetry(mutation.key)).toBe(false);
  });

  it("reconciled mutation 将 timeout 后已持久化的状态确认为 applied", async () => {
    let durable = false;
    let refreshed = 0;
    let applied = 0;
    const action = createReconciledMutation({
      refresh: async () => {
        refreshed++;
      },
      onChanged: () => applied++,
    });
    const result = await action.run({
      key: "message:one",
      prepare: () => ({ messageId: "message_stable" }),
      execute: async () => {
        durable = true;
        throw new Error("response lost");
      },
      applied: () => durable,
      okText: "sent",
    });

    expect(result).toBe("applied");
    expect(refreshed).toBe(1);
    expect(applied).toBe(1);
    expect(action.hasRetry("message:one")).toBe(false);
  });

  it("reconciled mutation 在 token 准备失败或 throw undefined 后仍解除 pending", async () => {
    const action = createReconciledMutation({
      refresh: async () => undefined,
      onChanged: vi.fn(),
    });
    expect(
      await action.run({
        key: "prepare-failure",
        prepare: () => {
          throw new Error("cannot prepare");
        },
        execute: async () => undefined,
        okText: "prepared",
      }),
    ).toBe("failed");
    expect(action.pending()).toBe(false);
    expect(
      await action.run({
        key: "undefined-error",
        prepare: () => ({}),
        execute: async () => {
          throw undefined;
        },
        okText: "executed",
      }),
    ).toBe("failed");
    expect(action.pending()).toBe(false);
  });
});
