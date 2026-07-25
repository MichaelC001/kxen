// TopAgentBar 实测：chip 渲染（Main 固定 + 每 agent run 一个）/ 点击切换选中 / 状态点样式
// / running chip 停止按钮 / 终态 chip 关闭按钮（dismiss 移出名单）。
import { render } from "solid-js/web";
import { afterEach, describe, expect, it, vi } from "vitest";
import TopAgentBar from "./TopAgentBar";
import { activeAgentFocus, setActiveAgentFocus, setActiveSessionId, setAgents } from "../lib/state";
import { flash } from "../lib/flash";
import type { AgentActivity } from "../lib/team";

const stopMock = vi.hoisted(() => ({
  calls: [] as Array<{ sid: string; name: string }>,
  result: true,
  error: null as Error | null,
}));
const dismissMock = vi.hoisted(() => ({
  calls: [] as Array<{ sid: string; name: string }>,
  result: true,
  error: null as Error | null,
  list: [] as AgentActivity[],
}));
vi.mock("../lib/team", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../lib/team")>();
  return {
    ...orig,
    agentsStop: async (sid: string, name: string) => {
      stopMock.calls.push({ sid, name });
      if (stopMock.error) throw stopMock.error;
      return stopMock.result;
    },
    agentsDismiss: async (sid: string, name: string) => {
      dismissMock.calls.push({ sid, name });
      if (dismissMock.error) throw dismissMock.error;
      return dismissMock.result;
    },
    agentsList: async () => dismissMock.list,
  };
});

function run(name: string, status: AgentActivity["status"]): AgentActivity {
  return {
    name,
    kind: "teammate",
    model: { provider: "anthropic", model: "claude-sonnet-4-5" },
    status,
    started_at: 0,
  };
}

function mount() {
  const dispose = render(() => <TopAgentBar />, document.body);
  const chips = () => [...document.querySelectorAll("[data-chip]")] as HTMLButtonElement[];
  const stops = () => [...document.querySelectorAll("[data-stop]")] as HTMLButtonElement[];
  const dismisses = () => [...document.querySelectorAll("[data-dismiss]")] as HTMLButtonElement[];
  return { dispose, chips, stops, dismisses, chip: (i: number) => chips()[i]! };
}

afterEach(() => {
  setAgents([]);
  setActiveAgentFocus("");
  setActiveSessionId("");
  stopMock.calls.length = 0;
  stopMock.result = true;
  stopMock.error = null;
  dismissMock.calls.length = 0;
  dismissMock.result = true;
  dismissMock.error = null;
  dismissMock.list = [];
  for (const m of flash.msgs()) flash.dismiss(m.id);
  document.body.innerHTML = "";
});

describe("TopAgentBar (webkit)", () => {
  it("Main 固定第一项，其后每个 agent run 一个 chip（含 model 小字）", () => {
    setAgents([run("builder", "working"), run("reviewer", "done")]);
    const { dispose, chips } = mount();
    const texts = chips().map((c) => c.textContent ?? "");
    expect(texts[0]).toBe("Main");
    expect(texts[1]).toContain("builder");
    expect(texts[1]).toContain("claude-sonnet-4-5");
    expect(texts[2]).toContain("reviewer");
    dispose();
  });

  it("点击 agent chip 选中，点击 Main 回到主会话", () => {
    setAgents([run("builder", "working")]);
    const { dispose, chip } = mount();
    chip(1).click();
    expect(activeAgentFocus()).toBe("builder");
    expect(chip(1).className).toContain("bg-[var(--bg-overlay)]");
    expect(chip(0).className).not.toContain("bg-[var(--bg-overlay)]");
    chip(0).click();
    expect(activeAgentFocus()).toBe("main");
    expect(chip(0).className).toContain("bg-[var(--bg-overlay)]");
    dispose();
  });

  it("working chip 状态点脉冲，failed 红点不脉冲", () => {
    setAgents([run("builder", "working"), run("reviewer", "failed")]);
    const { dispose, chip } = mount();
    const dot = (c: HTMLButtonElement) => c.querySelector("span")?.className ?? "";
    expect(dot(chip(1))).toContain("animate-pulse");
    expect(dot(chip(2))).toContain("bg-[var(--err)]");
    expect(dot(chip(2))).not.toContain("animate-pulse");
    dispose();
  });

  it("working chip 点停止：调 agents.stop 且选中态切回 main（done chip 不出按钮）", async () => {
    setAgents([run("builder", "working"), run("reviewer", "done")]);
    setActiveSessionId("s1");
    setActiveAgentFocus("builder");
    const { dispose, stops } = mount();
    expect(stops().length).toBe(1);
    stops()[0]!.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(stopMock.calls).toEqual([{ sid: "s1", name: "builder" }]);
    expect(activeAgentFocus()).toBe("main");
    dispose();
  });

  it("停非选中 chip 不动当前选中态", async () => {
    setAgents([run("builder", "working"), run("reviewer", "working")]);
    setActiveSessionId("s1");
    setActiveAgentFocus("reviewer");
    const { dispose, stops } = mount();
    stops()[0]!.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(stopMock.calls.map((c) => c.name)).toEqual(["builder"]);
    expect(activeAgentFocus()).toBe("reviewer");
    dispose();
  });

  it("agents.stop 返回 false：不切窗 + flashErr + chip 还原可点", async () => {
    setAgents([run("builder", "working")]);
    setActiveSessionId("s1");
    setActiveAgentFocus("builder");
    stopMock.result = false;
    const { dispose, stops, chip } = mount();
    stops()[0]!.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(stopMock.calls).toEqual([{ sid: "s1", name: "builder" }]);
    expect(activeAgentFocus()).toBe("builder");
    expect(flash.msgs().some((m) => m.kind === "err" && m.text.includes("builder"))).toBe(true);
    expect(chip(1).disabled).toBe(false);
    dispose();
  });

  it("agents.stop 异常：不切窗 + flashErr", async () => {
    setAgents([run("builder", "working")]);
    setActiveSessionId("s1");
    setActiveAgentFocus("builder");
    stopMock.error = new Error("io boom");
    const { dispose, stops } = mount();
    stops()[0]!.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(activeAgentFocus()).toBe("builder");
    expect(flash.msgs().some((m) => m.kind === "err" && m.text.includes("io boom"))).toBe(true);
    dispose();
  });

  it("点击停止乐观置灰：RPC 成功后靠轮询收敛摘灰", async () => {
    setAgents([run("builder", "working")]);
    setActiveSessionId("s1");
    const { dispose, stops, chip } = mount();
    stops()[0]!.click();
    expect(chip(1).disabled).toBe(true); // 乐观态立即生效（不等 RPC 返回）
    await new Promise((r) => setTimeout(r, 0));
    expect(chip(1).disabled).toBe(true); // 轮询未回仍置灰
    setAgents([run("builder", "shutdown")]); // 模拟轮询带回新状态
    await new Promise((r) => setTimeout(r, 0));
    expect(chip(1).disabled).toBe(false);
    dispose();
  });

  it("终态 chip（done/failed/shutdown）出关闭钮，working chip 不出", () => {
    setAgents([run("builder", "working"), run("reviewer", "done"), run("ghost", "shutdown")]);
    const { dispose, dismisses } = mount();
    expect(dismisses().length).toBe(2);
    expect(dismisses()[0]!.title).toContain("关闭 reviewer");
    expect(dismisses()[1]!.title).toContain("关闭 ghost");
    dispose();
  });

  it("关闭选中 chip：调 agents.dismiss，名单立即收敛 + 选中态切回 main", async () => {
    setAgents([run("builder", "working"), run("reviewer", "done")]);
    setActiveSessionId("s1");
    setActiveAgentFocus("reviewer");
    dismissMock.list = [run("builder", "working")]; // dismiss 后后端名单剩 builder
    const { dispose, dismisses, chips } = mount();
    dismisses()[0]!.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(dismissMock.calls).toEqual([{ sid: "s1", name: "reviewer" }]);
    expect(activeAgentFocus()).toBe("main");
    const texts = chips().map((c) => c.textContent ?? "");
    expect(texts.length).toBe(2);
    expect(texts[0]).toBe("Main");
    expect(texts[1]).toContain("builder");
    dispose();
  });

  it("agents.dismiss 返回 false：不切窗 + flashErr", async () => {
    setAgents([run("reviewer", "done")]);
    setActiveSessionId("s1");
    setActiveAgentFocus("reviewer");
    dismissMock.result = false;
    const { dispose, dismisses } = mount();
    dismisses()[0]!.click();
    await new Promise((r) => setTimeout(r, 0));
    expect(dismissMock.calls).toEqual([{ sid: "s1", name: "reviewer" }]);
    expect(activeAgentFocus()).toBe("reviewer");
    expect(flash.msgs().some((m) => m.kind === "err" && m.text.includes("reviewer"))).toBe(true);
    dispose();
  });
});
