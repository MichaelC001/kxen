// RightColumn 概览卡实测：preview 追 text/error/tool 事件（error 红字）、订阅自带 session topic。
import { render } from "solid-js/web";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import RightColumn from "./RightColumn";
import { activeAgentFocus, setActiveAgentFocus, setActiveSessionId, setAgents } from "../lib/state";
import type { AgentActivity, TranscriptEntry } from "../lib/team";

const mocks = vi.hoisted(() => ({
  transcript: vi.fn<(sid: string, name: string) => Promise<TranscriptEntry[]>>(),
  stop: vi.fn<(sid: string, name: string) => Promise<boolean>>(),
  dismiss: vi.fn<(sid: string, name: string) => Promise<boolean>>(),
  topicCalls: [] as string[][],
  handler: null as null | ((topic: string, payload: unknown) => void),
}));
vi.mock("../lib/team", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../lib/team")>();
  return {
    ...orig,
    agentsTranscript: mocks.transcript,
    agentsStop: mocks.stop,
    agentsDismiss: mocks.dismiss,
    agentsList: async () => [],
  };
});
vi.mock("../lib/chat", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../lib/chat")>();
  return {
    ...orig,
    onTopic: (topics: string[], handler: (topic: string, payload: unknown) => void) => {
      mocks.topicCalls.push(topics);
      mocks.handler = handler;
      return () => {};
    },
  };
});
// Dock 与概览卡无关（自带 RPC/订阅），整体替身避免基建噪音
vi.mock("./Dock", () => ({ default: () => <div data-dock-stub /> }));

function run(name: string, status: AgentActivity["status"]): AgentActivity {
  return { name, kind: "subagent", model: { provider: "p", model: "m" }, status, started_at: 0 };
}

const tick = () => new Promise((r) => setTimeout(r, 0));
const emit = (payload: unknown) => mocks.handler?.("", payload);
const previewEl = () => document.querySelector(".font-mono") as HTMLElement | null;

beforeEach(() => {
  mocks.transcript.mockReset().mockResolvedValue([]);
  mocks.stop.mockReset().mockResolvedValue(true);
  mocks.dismiss.mockReset().mockResolvedValue(true);
  mocks.topicCalls.length = 0;
  mocks.handler = null;
  setActiveSessionId("s1");
});

afterEach(() => {
  setAgents([]);
  setActiveSessionId("");
  setActiveAgentFocus("");
  document.body.innerHTML = "";
});

describe("RightColumn 概览卡", () => {
  it("初始 preview 取转录里最近的可展示条目（error 也算）", async () => {
    mocks.transcript.mockResolvedValue([
      { kind: "text", text: "旧正文" },
      { kind: "error", message: "io boom" },
    ]);
    setAgents([run("w", "failed")]);
    const dispose = render(() => <RightColumn />, document.body);
    await tick();
    expect(previewEl()?.textContent).toBe("io boom");
    expect(previewEl()?.className).toContain("text-[var(--err)]");
    dispose();
  });

  it("delta 订阅自带 session topic", async () => {
    setAgents([run("w", "working")]);
    const dispose = render(() => <RightColumn />, document.body);
    await tick();
    expect(mocks.topicCalls.at(-1)).toEqual(["llm.delta", "session:s1"]);
    dispose();
  });

  it("live：text 追加 / tool 替换 / error 红字替换，他 agent 他会话帧忽略", async () => {
    setAgents([run("w", "working")]);
    const dispose = render(() => <RightColumn />, document.body);
    await tick();
    emit({ agent: "w", session_id: "s1", kind: "tool_call", name: "exec", summary: "ls -la" });
    expect(previewEl()?.textContent).toBe("exec: ls -la");
    expect(previewEl()?.className).not.toContain("text-[var(--err)]");
    emit({ agent: "w", session_id: "s1", kind: "text", text: "流式" });
    emit({ agent: "w", session_id: "s1", kind: "text", text: "正文" });
    expect(previewEl()?.textContent).toBe("流式正文");
    emit({ agent: "w", session_id: "s1", kind: "error", message: "io boom" });
    expect(previewEl()?.textContent).toBe("io boom");
    expect(previewEl()?.className).toContain("text-[var(--err)]");
    // error 快照后 text 从干净起点续，不拼在红字尾巴上
    emit({ agent: "w", session_id: "s1", kind: "text", text: "后续" });
    expect(previewEl()?.textContent).toBe("后续");
    emit({ agent: "other", session_id: "s1", kind: "text", text: "别 agent" });
    emit({ agent: "w", session_id: "s2", kind: "text", text: "别会话" });
    expect(previewEl()?.textContent).toBe("后续");
    dispose();
  });
});

describe("RightColumn 管理钮", () => {
  const stops = () => [...document.querySelectorAll("[data-stop]")] as HTMLButtonElement[];
  const dismisses = () => [...document.querySelectorAll("[data-dismiss]")] as HTMLButtonElement[];

  it("running 卡出停止钮、终态卡出关闭钮（互不越界）", () => {
    setAgents([run("w", "working"), run("d", "done"), run("s", "shutdown")]);
    const dispose = render(() => <RightColumn />, document.body);
    expect(stops().length).toBe(1);
    expect(stops()[0]!.title).toBe("停止 w");
    expect(dismisses().length).toBe(2);
    expect(dismisses().map((b) => b.title)).toEqual(["关闭 d（移出名单）", "关闭 s（移出名单）"]);
    dispose();
  });

  it("点停止调 agents.stop；停选中卡切回 main，停后台卡不动焦点", async () => {
    setAgents([run("w", "working"), run("x", "working")]);
    setActiveAgentFocus("x");
    const dispose = render(() => <RightColumn />, document.body);
    await tick();
    stops()[0]!.click(); // 停后台的 w
    await tick();
    expect(mocks.stop).toHaveBeenCalledWith("s1", "w");
    expect(activeAgentFocus()).toBe("x");
    stops()[0]!.click(); // 停选中的 x（w 已置灰，剩 x 的钮）
    await tick();
    expect(mocks.stop).toHaveBeenCalledWith("s1", "x");
    expect(activeAgentFocus()).toBe("main");
    dispose();
  });

  it("点关闭调 agents.dismiss 并收敛名单", async () => {
    setAgents([run("d", "done")]);
    setActiveAgentFocus("d");
    const dispose = render(() => <RightColumn />, document.body);
    await tick();
    dismisses()[0]!.click();
    await tick();
    expect(mocks.dismiss).toHaveBeenCalledWith("s1", "d");
    expect(activeAgentFocus()).toBe("main");
    dispose();
  });
});
