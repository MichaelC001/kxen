// Trajectory 检视视图实测：记录表渲染、工具栏两级折叠与 Duration、搜索节流、尾部优先分页、
// Inspector 侧栏标签页、Chat Inspect 联动定位。Overview 交互在 TrajectoryOverview.test.tsx。
import { render } from "solid-js/web";
import { createSignal } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { StoredMessage } from "../lib/chat";
import type { InspectTarget } from "../lib/session-view";

const chatMock = vi.hoisted(() => ({ messages: [] as StoredMessage[] }));
vi.mock("../lib/chat", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../lib/chat")>();
  return { ...orig, sessionMessages: async () => chatMock.messages };
});
vi.mock("./Markdown", () => ({
  default: (p: { text: string }) => <div data-markdown="">{p.text}</div>,
}));

import TrajectoryView from "./TrajectoryView";

function msg(partial: Partial<StoredMessage> & Pick<StoredMessage, "id" | "role">): StoredMessage {
  return { session_id: "s1", parts: [], created_at: 0, ...partial };
}

function fixture(): StoredMessage[] {
  return [
    msg({ id: "m1", role: "user", created_at: 1000, parts: [{ type: "text", text: "修复登录" }] }),
    msg({
      id: "m2",
      role: "assistant",
      created_at: 2000,
      model: { provider: "anthropic", model: "claude" },
      parts: [
        {
          type: "tool_call",
          name: "grep",
          input: "authToken",
          args: { q: "authToken" },
          output: "src/auth.ts",
          started_at: 1100,
          finished_at: 1500,
        },
        { type: "text", text: "找到了" },
      ],
    }),
    msg({
      id: "m3",
      role: "assistant",
      created_at: 9000,
      parts: [{ type: "text", text: "完成" }],
      stats: {
        ttft_ms: 500,
        duration_ms: 3000,
        input_tokens: 10,
        output_tokens: 5,
        tokens_per_sec: 2,
        usage_complete: true,
      },
    }),
  ];
}

function mount(messages: StoredMessage[], options?: { focus?: InspectTarget }) {
  chatMock.messages = messages;
  const [focus, setFocus] = createSignal<InspectTarget | null>(options?.focus ?? null);
  const dispose = render(
    () => (
      <TrajectoryView
        sessionId={() => "s1"}
        active={() => true}
        streaming={() => false}
        focus={focus}
        onFocusConsumed={() => setFocus(null)}
      />
    ),
    document.body,
  );
  return { dispose, setFocus };
}

const rows = () => [...document.body.querySelectorAll("[data-testid='trajectory-row']")];
const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

afterEach(() => {
  document.body.innerHTML = "";
});

describe("TrajectoryView 记录表", () => {
  it("渲染 #N / 类型 / 内容三列，tool call+result 合并一行并显示耗时", async () => {
    const { dispose } = mount(fixture());
    await vi.waitFor(() => expect(rows().length).toBe(4));
    const text = document.body.textContent ?? "";
    expect(text).toContain("#0");
    expect(text).toContain("user");
    expect(text).toContain("修复登录");
    expect(text).toContain("grep authToken"); // 摘要同行
    expect(text).toContain("400ms"); // 1500-1100 实测耗时
    expect(text).toContain("3.0s"); // 收尾消息 stats duration
    dispose();
  });

  it("Collapse turns：折叠轮次保留首行 + 步骤/工具计数，点击展开", async () => {
    const { dispose } = mount(fixture());
    await vi.waitFor(() => expect(rows().length).toBe(4));
    const buttons = [...document.body.querySelectorAll("button")];
    const collapse = buttons.find((b) => b.textContent === "Collapse turns")!;
    collapse.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await vi.waitFor(() =>
      expect(
        document.body.querySelectorAll("[data-testid='trajectory-turn-collapsed']").length,
      ).toBe(1),
    );
    expect(document.body.textContent).toContain("4 步骤 · 1 工具调用");
    expect(document.body.textContent).not.toContain("找到了");
    document.body
      .querySelector("[data-testid='trajectory-turn-collapsed']")!
      .dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await vi.waitFor(() => expect(document.body.textContent).toContain("找到了"));
    dispose();
  });

  it("Collapse calls 收起工具摘要只留名字；Duration 切换隐藏耗时列", async () => {
    const { dispose } = mount(fixture());
    await vi.waitFor(() => expect(rows().length).toBe(4));
    const buttons = [...document.body.querySelectorAll("button")];
    buttons
      .find((b) => b.textContent === "Collapse calls")!
      .dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await vi.waitFor(() => expect(document.body.textContent).not.toContain("grep authToken"));
    expect(
      rows().some((r) => r.textContent?.includes("grep") && !r.textContent?.includes("authToken")),
    ).toBe(true);
    buttons
      .find((b) => b.textContent === "Duration")!
      .dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await vi.waitFor(() => expect(document.body.textContent).not.toContain("400ms"));
    dispose();
  });

  it("搜索节流提交，覆盖工具参数字段", async () => {
    const { dispose } = mount(fixture());
    await vi.waitFor(() => expect(rows().length).toBe(4));
    const input = document.body.querySelector<HTMLInputElement>(
      "[data-testid='trajectory-search']",
    )!;
    input.value = "authToken";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    await sleep(250);
    await vi.waitFor(() => expect(rows().length).toBe(1));
    input.value = "";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    await sleep(250);
    await vi.waitFor(() => expect(rows().length).toBe(4));
    dispose();
  });

  it("尾部优先分页：只挂最新一页，顶部控件加载更早一页", async () => {
    const many: StoredMessage[] = Array.from({ length: 150 }, (_, i) =>
      msg({
        id: `u${i}`,
        role: "user",
        created_at: i,
        parts: [{ type: "text", text: `消息第${i}条` }],
      }),
    );
    const { dispose } = mount(many);
    await vi.waitFor(() => expect(rows().length).toBe(100));
    expect(document.body.textContent).toContain("消息第149条"); // 尾部
    expect(document.body.textContent).not.toContain("消息第10条");
    document.body
      .querySelector("[data-testid='trajectory-load-earlier']")!
      .dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await vi.waitFor(() => expect(document.body.textContent).toContain("消息第10条"));
    dispose();
  });

  it("选中记录开 Inspector：工具给 Timing（含实测耗时），消息给模型字段", async () => {
    const { dispose } = mount(fixture());
    await vi.waitFor(() => expect(rows().length).toBe(4));
    rows()[1]!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await vi.waitFor(() =>
      expect(document.body.querySelector("[data-testid='trajectory-inspector']")).toBeTruthy(),
    );
    expect(document.body.textContent).toContain("Schema");
    const timing = [...document.body.querySelectorAll("button")].find(
      (b) => b.textContent === "Timing",
    )!;
    timing.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await vi.waitFor(() => expect(document.body.textContent).toContain("400ms"));
    // 消息记录：模型标签页显示 provider/model
    rows()[2]!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    const model = [...document.body.querySelectorAll("button")].find(
      (b) => b.textContent === "模型",
    )!;
    model.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await vi.waitFor(() => expect(document.body.textContent).toContain("anthropic"));
    dispose();
  });

  it("Chat Inspect 联动：focus 定位选中对应记录并打开检查器", async () => {
    const { dispose } = mount(fixture(), { focus: { messageId: "m2", partIndex: 0 } });
    await vi.waitFor(() =>
      expect(document.body.querySelector("[data-testid='trajectory-inspector']")).toBeTruthy(),
    );
    expect(document.body.textContent).toContain("#1 tool");
    dispose();
  });

  it("存量记录缺计时/模型：字段留空或显式未知，不编造", async () => {
    const { dispose } = mount([
      msg({
        id: "m1",
        role: "assistant",
        parts: [{ type: "tool_call", name: "read", input: "a", output: "b" }],
      }),
    ]);
    await vi.waitFor(() => expect(rows().length).toBe(1));
    expect(document.body.textContent).not.toContain("0ms");
    rows()[0]!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await vi.waitFor(() => expect(document.body.textContent).toContain("Schema"));
    const timing = [...document.body.querySelectorAll("button")].find(
      (b) => b.textContent === "Timing",
    )!;
    timing.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await vi.waitFor(() => expect(document.body.textContent).toContain("未知"));
    dispose();
  });
});
