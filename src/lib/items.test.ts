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
