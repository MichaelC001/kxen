// workflow phase 上屏文案（块三）：有 index/total 用 `phase i/N · title`（修双冒号），无则 `phase: xxx`
import { createSignal } from "solid-js";
import { describe, expect, it } from "vitest";
import { applyStreamEvent } from "./session-events";
import type { Item } from "./items";
import type { OrbState } from "./orb";

function setup() {
  const [items, setItems] = createSignal<Item[]>([]);
  const [, setOrbPhase] = createSignal<OrbState>("thinking");
  const deps = { setItems, setOrbPhase, scroll: () => {} };
  return { deps, items, last: () => items().at(-1) };
}

describe("applyStreamEvent phase 分支", () => {
  it("有 index/total 渲染为 phase i/N · title（不再有第二个冒号）", () => {
    const { deps, last } = setup();
    applyStreamEvent({ kind: "phase", name: "业务补齐", index: 2, total: 10 }, deps);
    expect(last()).toEqual({ kind: "phase", name: "phase 2/10 · 业务补齐" });
  });

  it("无 index 保持 phase: xxx", () => {
    const { deps, last } = setup();
    applyStreamEvent({ kind: "phase", name: "scan" }, deps);
    expect(last()).toEqual({ kind: "phase", name: "phase: scan" });
  });
});

describe("applyStreamEvent tool_result 分支", () => {
  it("完整 output 填入结果（流式展开区透传）", () => {
    const { deps, items } = setup();
    applyStreamEvent({ kind: "tool_call", name: "exec", summary: "ls" }, deps);
    applyStreamEvent(
      { kind: "tool_result", name: "exec", summary: "done", output: "file1\nfile2" },
      deps,
    );
    const tool = items().find((it) => it.kind === "tool");
    expect(tool && "result" in tool ? tool.result : undefined).toBe("file1\nfile2");
  });

  it("output 缺省回退一行摘要", () => {
    const { deps, items } = setup();
    applyStreamEvent({ kind: "tool_call", name: "exec", summary: "ls" }, deps);
    applyStreamEvent({ kind: "tool_result", name: "exec", summary: "done" }, deps);
    const tool = items().find((it) => it.kind === "tool");
    expect(tool && "result" in tool ? tool.result : undefined).toBe("done");
  });
});

describe("applyStreamEvent approval_resolved 分支", () => {
  it("等待中的审批卡置失效", () => {
    const { deps, items } = setup();
    applyStreamEvent(
      { kind: "approval", name: "approval", approvalId: "a1", command: "rm x", reason: "r" },
      deps,
    );
    applyStreamEvent(
      { kind: "approval_resolved", name: "approval", approvalId: "a1", outcome: "timeout" },
      deps,
    );
    const card = items().find((it) => it.kind === "approval");
    expect(card && "resolved" in card ? card.resolved : undefined).toBe("timeout");
  });
});
