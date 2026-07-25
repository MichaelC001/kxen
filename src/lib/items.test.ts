// 通知类 user 消息（[teammate x] / [task notification] 前缀）解析来源小标；普通消息不带。
import { describe, expect, it } from "vitest";
import { toItems, userSource, type MsgItem } from "./items";
import type { StoredMessage } from "./chat";

function stored(role: "user" | "assistant", text: string, id = "m1"): StoredMessage {
  return { id, session_id: "s1", role, parts: [{ type: "text", text }], created_at: 0 };
}

describe("toItems 通知来源小标", () => {
  it("[teammate x] 前缀的 user 消息带 teammate 来源，内容原样渲染", () => {
    const items = toItems([stored("user", "[teammate builder] 已完成重构")]);
    expect(items[0]).toMatchObject({
      kind: "msg",
      role: "user",
      content: "[teammate builder] 已完成重构",
      source: "teammate builder",
    });
  });

  it("[task notification] 前缀的 user 消息带 task notification 来源", () => {
    const items = toItems([
      stored("user", "[task notification] agent a (execution) finished:\ndone"),
    ]);
    expect(items[0]).toMatchObject({ kind: "msg", role: "user", source: "task notification" });
  });

  it("普通用户消息与 assistant 消息（即使文本同前缀）不带来源", () => {
    const items = toItems([
      stored("user", "帮我看看", "m1"),
      stored("assistant", "[teammate x] 我转述给你", "m2"),
    ]);
    expect((items[0] as MsgItem).source).toBeUndefined();
    expect((items[1] as MsgItem).source).toBeUndefined();
  });

  it("userSource 直判", () => {
    expect(userSource("[teammate w] done")).toBe("teammate w");
    expect(userSource("[task notification] agent a failed:\nboom")).toBe("task notification");
    expect(userSource("[teammate] 缺名不算")).toBeUndefined();
    expect(userSource("普通口信")).toBeUndefined();
  });
});

describe("toItems 落盘审批决定（Part approval）", () => {
  function storedApproval(decision: string): StoredMessage {
    return {
      id: "m1",
      session_id: "s1",
      role: "assistant",
      parts: [{ type: "approval", command: "rm -rf x", reason: "危险", decision }],
      created_at: 0,
    };
  }

  it("allow/deny/timeout/cancel 渲染为已决历史卡（无 approvalId，按钮不出现）", () => {
    const cases = [
      ["allow", "allowed"],
      ["deny", "denied"],
      ["timeout", "timeout"],
      ["cancel", "cancelled"],
    ] as const;
    for (const [decision, resolved] of cases) {
      const items = toItems([storedApproval(decision)]);
      expect(items[0]).toMatchObject({
        kind: "approval",
        approvalId: "",
        command: "rm -rf x",
        reason: "危险",
        resolved,
      });
    }
  });

  it("未知 decision 按 expired 兜底（不冒充用户决定）", () => {
    const items = toItems([storedApproval("bogus")]);
    expect(items[0]).toMatchObject({ kind: "approval", resolved: "expired" });
  });

  it("落盘卡与文字消息按时序混排", () => {
    const items = toItems([
      stored("user", "帮我删一下", "m0"),
      storedApproval("allow"),
      stored("assistant", "已删除", "m2"),
    ]);
    expect(items.map((i) => i.kind)).toEqual(["msg", "approval", "msg"]);
  });
});
