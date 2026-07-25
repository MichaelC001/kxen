// TopAgentBar 实测：chip 渲染（Main 固定 + 每 agent run 一个）/ 点击切换选中 / 状态点样式 / running chip 停止按钮。
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
vi.mock("../lib/team", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../lib/team")>();
  return {
    ...orig,
    agentsStop: async (sid: string, name: string) => {
      stopMock.calls.push({ sid, name });
      if (stopMock.error) throw stopMock.error;
      return stopMock.result;
    },
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
  return { dispose, chips, stops, chip: (i: number) => chips()[i]! };
}

afterEach(() => {
  setAgents([]);
  setActiveAgentFocus("");
  setActiveSessionId("");
  stopMock.calls.length = 0;
  stopMock.result = true;
  stopMock.error = null;
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
});
