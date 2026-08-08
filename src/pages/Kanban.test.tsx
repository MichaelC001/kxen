// 看板页关键交互回归：approve/reject/comment/retry/policy 走正确 RPC 字面量与参数，
// snapshot 渲染列与卡，policy 表单把分钟数序列化为 expires_at_ms。
// 只 mock client 传输层：chat-ops wrapper 真实执行，字面量以 rpc_contract 门禁同款方式核对。
import { render } from "solid-js/web";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { JSX } from "solid-js";
import type { KanbanSnapshot } from "../lib/chat";

const h = vi.hoisted(() => ({
  rpc: vi.fn(async (_method: string, _params?: unknown): Promise<unknown> => null),
  on: vi.fn((_handler: (payload: unknown) => void) => vi.fn()),
  stream: vi.fn(),
  resync: new Set<() => void>(),
}));

h.stream.mockImplementation(() => ({ on: h.on }));

vi.mock("../lib/client", () => ({
  client: {
    rpc: h.rpc,
    stream: h.stream,
    onResync: (cb: () => void) => {
      h.resync.add(cb);
      return () => h.resync.delete(cb);
    },
  },
}));

// 测试无路由装配：A 桩成普通锚，params/searchParams 固定注入
vi.mock("@solidjs/router", () => ({
  A: (props: { href: string; class?: string; children?: JSX.Element }) => (
    <a href={props.href} class={props.class}>
      {props.children}
    </a>
  ),
  useParams: () => ({ board: "board_1" }),
  useSearchParams: () => [{ workspace: "/ws/a" }],
}));

import Kanban from "./Kanban";

const flush = () => new Promise((r) => setTimeout(r, 0));

const SNAPSHOT: KanbanSnapshot = {
  board_id: "board_1",
  title: "交付板",
  columns: [
    {
      id: "review",
      title: "待验证",
      on_enter: { kind: "human_gate" },
      transitions: { on_success: "done", on_failure: "implementing" },
    },
    {
      id: "implementing",
      title: "实现中",
      on_enter: { kind: "agent_run", agent: "execution" },
      transitions: { on_success: "review", on_failure: "review" },
      wip_limit: 3,
    },
    { id: "done", title: "完成", on_enter: { kind: "none" }, transitions: {} },
  ],
  cards: {
    card_1: {
      id: "card_1",
      column_id: "review",
      title: "加登录",
      body: "接上 OAuth",
      status: "waiting_human",
      created_at: 1,
      updated_at: 2,
      comments: [{ author: "agent", body: "已完成实现", at: 1 }],
    },
    card_2: {
      id: "card_2",
      column_id: "implementing",
      title: "修崩溃",
      body: "",
      status: "blocked",
      created_at: 1,
      updated_at: 3,
      block_reason: "run timeout",
      comments: [],
    },
    card_3: {
      id: "card_3",
      column_id: "done",
      title: "已上线",
      body: "",
      status: "ready",
      created_at: 1,
      updated_at: 4,
      comments: [],
    },
  },
  runs: {},
  agents: {},
  policy: null,
  seq: 9,
};

const btnByText = (text: string) =>
  [...document.body.querySelectorAll("button")].find((el) => el.textContent?.includes(text));

beforeEach(() => {
  h.rpc.mockImplementation(async (method: string) =>
    method === "kanban.snapshot" ? SNAPSHOT : {},
  );
});

afterEach(() => {
  document.body.innerHTML = "";
  h.rpc.mockReset();
  h.on.mockClear();
  h.resync.clear();
});

describe("Kanban snapshot 渲染", () => {
  it("渲染列头（含 WIP 计数）与卡片，点卡片展开详情", async () => {
    const dispose = render(() => <Kanban />, document.body);
    await flush();
    expect(h.rpc).toHaveBeenCalledWith("kanban.snapshot", { workspace: "/ws/a", board: "board_1" });
    expect(document.body.textContent).toContain("交付板");
    expect(document.body.textContent).toContain("待验证");
    expect(document.body.textContent).toContain("1/3"); // implementing 列 wip_limit=3
    expect(document.body.textContent).toContain("加登录");
    expect(document.body.textContent).toContain("修崩溃");
    expect(document.body.textContent).not.toContain("已完成实现"); // 详情未展开

    btnByText("加登录")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();
    expect(document.body.textContent).toContain("接上 OAuth");
    expect(document.body.textContent).toContain("已完成实现");
    dispose();
  });

  it("订阅 kanban:<board> topic 并注册 resync，卸载后注销", async () => {
    const dispose = render(() => <Kanban />, document.body);
    await flush();
    expect(h.stream).toHaveBeenCalledWith(["kanban:board_1"]);
    expect(h.resync.size).toBe(1);
    dispose();
    expect(h.resync.size).toBe(0);
  });

  it("首载失败显示错误态与重试（与真空区分）", async () => {
    h.rpc.mockImplementation(async () => {
      throw new Error("connection lost");
    });
    const dispose = render(() => <Kanban />, document.body);
    await flush();
    expect(document.body.textContent).toContain("加载看板失败");
    expect(document.body.textContent).not.toContain("待验证");
    h.rpc.mockImplementation(async (method: string) =>
      method === "kanban.snapshot" ? SNAPSHOT : {},
    );
    btnByText("重试")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();
    expect(document.body.textContent).toContain("加登录");
    dispose();
  });
});

describe("Kanban 人工动作", () => {
  it("待审卡：通过/打回调 kanban.card_move success/failure", async () => {
    const dispose = render(() => <Kanban />, document.body);
    await flush();
    btnByText("加登录")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();

    btnByText("通过")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();
    expect(h.rpc).toHaveBeenCalledWith("kanban.card_move", {
      workspace: "/ws/a",
      board: "board_1",
      card_id: "card_1",
      outcome: "success",
    });

    btnByText("打回")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();
    expect(h.rpc).toHaveBeenCalledWith("kanban.card_move", {
      workspace: "/ws/a",
      board: "board_1",
      card_id: "card_1",
      outcome: "failure",
    });
    dispose();
  });

  it("阻塞卡：重试调 kanban.run_start", async () => {
    const dispose = render(() => <Kanban />, document.body);
    await flush();
    btnByText("修崩溃")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();
    btnByText("重试")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();
    expect(h.rpc).toHaveBeenCalledWith("kanban.run_start", {
      workspace: "/ws/a",
      board: "board_1",
      card_id: "card_2",
    });
    dispose();
  });

  it("评论调 kanban.card_comment 并清空输入", async () => {
    const dispose = render(() => <Kanban />, document.body);
    await flush();
    btnByText("加登录")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();

    const textarea = document.body.querySelector("textarea");
    expect(textarea).toBeTruthy();
    textarea!.value = "人工复核通过";
    textarea!.dispatchEvent(new Event("input", { bubbles: true }));
    await flush();
    const submitBtn = [...document.body.querySelectorAll("button")].find(
      (el) => el.textContent?.trim() === "评论",
    );
    submitBtn?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();
    expect(h.rpc).toHaveBeenCalledWith("kanban.card_comment", {
      workspace: "/ws/a",
      board: "board_1",
      card_id: "card_1",
      body: "人工复核通过",
    });
    dispose();
  });

  it("新建卡片调 kanban.card_create（标题 trim，正文随参）", async () => {
    const dispose = render(() => <Kanban />, document.body);
    await flush();
    btnByText("新建卡片")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();

    const title = document.body.querySelector("input[aria-label='卡片标题']") as HTMLInputElement;
    const body = document.body.querySelector(
      "textarea[aria-label='卡片正文']",
    ) as HTMLTextAreaElement;
    title.value = " 补文档 ";
    title.dispatchEvent(new Event("input", { bubbles: true }));
    body.value = "README 加看板章节";
    body.dispatchEvent(new Event("input", { bubbles: true }));
    await flush();
    btnByText("创建")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();
    expect(h.rpc).toHaveBeenCalledWith("kanban.card_create", {
      workspace: "/ws/a",
      board: "board_1",
      title: "补文档",
      body: "README 加看板章节",
    });
    dispose();
  });

  it("授权表单：前缀按行拆分，分钟数序列化为 expires_at_ms", async () => {
    const dispose = render(() => <Kanban />, document.body);
    await flush();
    btnByText("授权")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();

    const allowlist = document.body.querySelector(
      "textarea[aria-label='授权命令前缀']",
    ) as HTMLTextAreaElement;
    const uses = document.body.querySelector(
      "input[aria-label='最大自动放行次数']",
    ) as HTMLInputElement;
    const mins = document.body.querySelector(
      "input[aria-label='授权时限分钟数']",
    ) as HTMLInputElement;
    allowlist.value = "cargo test\n\ncargo clippy\n";
    allowlist.dispatchEvent(new Event("input", { bubbles: true }));
    uses.value = "5";
    uses.dispatchEvent(new Event("input", { bubbles: true }));
    mins.value = "30";
    mins.dispatchEvent(new Event("input", { bubbles: true }));
    await flush();

    const before = Date.now();
    btnByText("保存授权")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();
    const call = h.rpc.mock.calls.find(([method]) => method === "kanban.policy_set");
    expect(call).toBeTruthy();
    const params = call![1] as {
      workspace: string;
      board: string;
      policy: { allowlist: string[]; max_uses: number; expires_at_ms: number };
    };
    expect(params.workspace).toBe("/ws/a");
    expect(params.board).toBe("board_1");
    expect(params.policy.allowlist).toEqual(["cargo test", "cargo clippy"]); // 空行丢弃
    expect(params.policy.max_uses).toBe(5);
    expect(params.policy.expires_at_ms).toBeGreaterThanOrEqual(before + 30 * 60_000);
    expect(params.policy.expires_at_ms).toBeLessThanOrEqual(Date.now() + 30 * 60_000);
    dispose();
  });
});
