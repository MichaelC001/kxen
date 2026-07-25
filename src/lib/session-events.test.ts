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
  return { deps, last: () => items().at(-1) };
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
