// 迭代级持久化（crates/kxen-core/src/ws/llm_task/turn_persistence.rs）后的时间线归并：
// 同一 run 的连续 Assistant 迭代消息 + 收尾消息归并为一个视觉回合；存量打包消息渲染不变。
import { describe, expect, it } from "vitest";
import { toItems } from "./items";
import type { ModelIdentity, StoredMessage, StoredPart } from "./chat";

const MODEL: ModelIdentity = { provider: "anthropic", model: "claude-sonnet-4-6" };

function msg(
  id: string,
  role: "user" | "assistant",
  parts: StoredPart[],
  model?: ModelIdentity,
): StoredMessage {
  return { id, session_id: "s1", role, parts, created_at: 0, ...(model ? { model } : {}) };
}

function iteration(stream: string, turn: number, parts: StoredPart[]): StoredMessage {
  return msg(`${stream}:t${turn}`, "assistant", parts, MODEL);
}

function call(name: string, output: string): StoredPart {
  // id 为 provider call id：前端不消费但反序列化需兼容；output 全量内联（用例用 20k 覆盖大输出）
  return { type: "tool_call", name, input: name, output, id: `call_${name}_1` };
}

describe("toItems 迭代消息归并为一个视觉回合", () => {
  it("连续迭代消息 + 收尾消息：工具卡按序内嵌，全部文本进末尾单条气泡", () => {
    const items = toItems([
      msg("u1", "user", [{ type: "text", text: "帮我改文件" }]),
      iteration("run-1-0001", 1, [
        { type: "text", text: "先看一下" },
        call("read", "x".repeat(20_000)),
        call("glob", "src/a.ts"),
      ]),
      iteration("run-1-0001", 2, [
        { type: "text", text: "改这里" },
        call("write", "wrote 10 bytes"),
      ]),
      msg(
        "msg_final",
        "assistant",
        [
          { type: "reasoning", text: "思考" },
          { type: "text", text: "已完成" },
        ],
        MODEL,
      ),
    ]);

    expect(items.map((i) => i.kind)).toEqual(["msg", "tool", "tool", "tool", "msg"]);
    expect(items[1]).toMatchObject({ name: "read", result: "x".repeat(20_000) });
    expect(items[4]).toEqual({
      kind: "msg",
      role: "assistant",
      content: "先看一下\n改这里\n已完成",
      messageId: "msg_final",
      reasoning: "思考",
      model: MODEL,
    });
  });

  it("收尾消息只有 reasoning（迭代已落盘、无最终文本）也并入回合", () => {
    const items = toItems([
      iteration("run-2-0001", 1, [call("read", "data")]),
      msg("msg_final", "assistant", [{ type: "reasoning", text: "只思考没说话" }]),
    ]);

    expect(items).toEqual([
      { kind: "tool", name: "read", call: "read", args: undefined, result: "data" },
      {
        kind: "msg",
        role: "assistant",
        content: "",
        messageId: "msg_final",
        reasoning: "只思考没说话",
        model: MODEL,
      },
    ]);
  });

  it("回合内审批消息出已决历史卡但不打断回合", () => {
    const items = toItems([
      iteration("run-3-0001", 1, [call("exec", "need approval")]),
      msg("msg_appr", "assistant", [
        { type: "approval", command: "rm -rf x", reason: "危险", decision: "allow" },
      ]),
      iteration("run-3-0001", 2, [call("exec", "done")]),
      msg("msg_final", "assistant", [{ type: "text", text: "搞定" }]),
    ]);

    expect(items.map((i) => i.kind)).toEqual(["tool", "approval", "tool", "msg"]);
    expect(items[1]).toMatchObject({ kind: "approval", resolved: "allowed" });
    expect(items[3]).toMatchObject({ kind: "msg", content: "搞定", messageId: "msg_final" });
  });

  it("不同 stream 的迭代消息不归并", () => {
    const items = toItems([
      iteration("run-4-0001", 1, [{ type: "text", text: "甲轮" }, call("read", "a")]),
      iteration("run-4-0002", 1, [{ type: "text", text: "乙轮" }, call("read", "b")]),
    ]);

    expect(items.map((i) => i.kind)).toEqual(["tool", "msg", "tool", "msg"]);
    expect(items[1]).toMatchObject({ content: "甲轮", messageId: "run-4-0001:t1" });
    expect(items[3]).toMatchObject({ content: "乙轮", messageId: "run-4-0002:t1" });
  });
});

describe("toItems 崩溃无尾回合", () => {
  it("迭代消息带文本但无收尾消息：回合气泡用最后一条迭代消息 id", () => {
    const items = toItems([
      msg("u1", "user", [{ type: "text", text: "帮我改文件" }]),
      iteration("run-5-0001", 1, [{ type: "text", text: "先看一下" }, call("read", "data")]),
      iteration("run-5-0001", 2, [call("write", "wrote 10 bytes")]),
      msg("u2", "user", [{ type: "text", text: "继续" }]),
    ]);

    expect(items.map((i) => i.kind)).toEqual(["msg", "tool", "tool", "msg", "msg"]);
    expect(items[3]).toEqual({
      kind: "msg",
      role: "assistant",
      content: "先看一下",
      messageId: "run-5-0001:t2",
      model: MODEL,
    });
    expect(items[4]).toMatchObject({ role: "user", content: "继续" });
  });

  it("纯工具无尾回合：不留空白气泡", () => {
    const items = toItems([
      msg("u1", "user", [{ type: "text", text: "跑一下" }]),
      iteration("run-6-0001", 1, [call("exec", "ok")]),
    ]);

    expect(items.map((i) => i.kind)).toEqual(["msg", "tool"]);
  });
});

describe("toItems 存量打包消息渲染不变", () => {
  it("单条 Reasoning+ToolCall×N+Text 打包消息：工具卡在前，气泡在末且挂 reasoning", () => {
    const packed: StoredMessage = {
      id: "msg_packed",
      session_id: "s1",
      role: "assistant",
      created_at: 0,
      model: MODEL,
      parts: [
        { type: "reasoning", text: "想" },
        { type: "tool_call", name: "shell", input: "pwd", args: { cwd: "/repo" }, output: "ok" },
        { type: "tool_call", name: "read", input: { path: "a" }, output: "" },
        { type: "text", text: "答案" },
      ],
    };

    expect(toItems([msg("u1", "user", [{ type: "text", text: "问" }]), packed])).toEqual([
      { kind: "msg", role: "user", content: "问", messageId: "u1", source: undefined },
      { kind: "tool", name: "shell", call: "pwd", args: '{\n  "cwd": "/repo"\n}', result: "ok" },
      { kind: "tool", name: "read", call: '{"path":"a"}', args: undefined, result: undefined },
      {
        kind: "msg",
        role: "assistant",
        content: "答案",
        messageId: "msg_packed",
        reasoning: "想",
        model: MODEL,
      },
    ]);
  });

  it("存量连续纯文本 Assistant 消息不互相归并", () => {
    const items = toItems([
      msg("a1", "assistant", [{ type: "text", text: "第一条" }]),
      msg("a2", "assistant", [{ type: "text", text: "第二条" }]),
    ]);

    expect(items.map((i) => (i.kind === "msg" ? i.content : i.kind))).toEqual(["第一条", "第二条"]);
  });
});
