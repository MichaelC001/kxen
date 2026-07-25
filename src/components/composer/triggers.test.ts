// detectTrigger 纯函数 + buildItems 动作契约：
// slash 收窄行首（后端只展开消息开头命令）、@/# 任意位置、apply 只调一个动作。
import { describe, expect, it, vi } from "vitest";
import { buildItems, detectTrigger, type PopupActions } from "./triggers";

vi.mock("../../lib/chat", () => ({
  fsComplete: async (query: string) =>
    [{ path: "src/App.tsx", kind: "file" }].filter((e) => e.path.includes(query)),
}));

describe("detectTrigger", () => {
  it("行首触发：\\n 后的 @ / / # 全部生效，消息开头 / 也生效", () => {
    expect(detectTrigger("第一行\n@sr", 7)).toEqual({ kind: "at", start: 4, query: "sr" });
    expect(detectTrigger("a\n/doc", 6)).toEqual({ kind: "slash", start: 2, query: "doc" });
    expect(detectTrigger("a\n#note", 7)).toEqual({ kind: "hash", start: 2, query: "note" });
    expect(detectTrigger("a\n/", 3)).toEqual({ kind: "slash", start: 2, query: "" });
    expect(detectTrigger("/doc", 4)).toEqual({ kind: "slash", start: 0, query: "doc" });
  });

  it("光标紧贴 \n（触发符在光标处而非其前）不触发", () => {
    // 旧 \n 特判的假阳性：光标还没越过 / 就报了 slash
    expect(detectTrigger("\n/doc", 1)).toBeNull();
  });

  it("slash 中段不触发：空白/全角括号前界同样拒绝（后端只展开消息开头命令）", () => {
    expect(detectTrigger("帮我 /doc", 7)).toBeNull();
    expect(detectTrigger("（/doc", 5)).toBeNull();
    expect(detectTrigger("a /doc b", 6)).toBeNull();
  });

  it("全角边界：@/# 全角空格与（【｛ 后可触发", () => {
    expect(detectTrigger("你好　@sr", 6)).toEqual({ kind: "at", start: 3, query: "sr" });
    expect(detectTrigger("【#note", 6)).toEqual({ kind: "hash", start: 1, query: "note" });
    expect(detectTrigger("（@a", 3)).toEqual({ kind: "at", start: 1, query: "a" });
    expect(detectTrigger("｛@a", 3)).toEqual({ kind: "at", start: 1, query: "a" });
  });

  it("query 不跨全角空格", () => {
    expect(detectTrigger("@foo　bar", 8)).toBeNull();
  });

  it("@/# 原有边界与拒绝不变", () => {
    expect(detectTrigger("src/comp", 8)).toBeNull();
    expect(detectTrigger("a@b", 3)).toBeNull();
  });
});

describe("buildItems apply 契约", () => {
  // onChip 实现方（TextComposer）内部已删触发词：apply 连调第二个动作会在新文本上再删一次
  function recorder() {
    const calls: string[] = [];
    const actions: PopupActions = {
      onChip: () => calls.push("chip"),
      onPlainInsert: () => calls.push("insert"),
    };
    return { calls, actions };
  }

  it("at 条目 apply 只调 onChip 一次", async () => {
    const { calls, actions } = recorder();
    const items = await buildItems({ kind: "at", start: 0, query: "App" }, [], actions);
    expect(items.length).toBeGreaterThan(0);
    items[0]!.apply();
    expect(calls).toEqual(["chip"]);
  });

  it("knowledge 条目 apply 只调 onChip 一次", async () => {
    const { calls, actions } = recorder();
    const items = await buildItems({ kind: "hash", start: 0, query: "" }, [], actions);
    expect(items.length).toBeGreaterThan(0);
    items[0]!.apply();
    expect(calls).toEqual(["chip"]);
  });

  it("slash 条目 apply 只调 onPlainInsert 一次", async () => {
    const { calls, actions } = recorder();
    const items = await buildItems(
      { kind: "slash", start: 0, query: "doc" },
      [{ name: "doctor", description: "环境自检", kind: "builtin" }],
      actions,
    );
    expect(items.length).toBeGreaterThan(0);
    items[0]!.apply();
    expect(calls).toEqual(["insert"]);
  });
});
