// StatusBar 会话切换即时刷新：activeSessionId 变化立即按新会话重拉 statusline，
// 不再等 3s 轮询（tokens/ctx/model 最长 3s 显示上一会话数据的回归点）。
import { render } from "solid-js/web";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { StatuslineReport } from "../lib/chat";

const h = vi.hoisted(() => ({
  statusline: vi.fn(
    async (_sid: string): Promise<StatuslineReport> => ({
      items: ["tokens", "ctx", "model"],
      workdir: "/tmp",
      git_branch: "main",
      goal: null,
      tasks_running: 0,
      tokens: { input: 1, output: 2 },
      ctx_pct: 3,
      model: "xai/grok",
    }),
  ),
}));

vi.mock("../lib/chat", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../lib/chat")>();
  return { ...orig, statusline: h.statusline };
});

// modelsCatalog 走 RPC：桩为空目录（ctx 窗文案不在本用例断言内）
vi.mock("../lib/models", async (importOriginal) => {
  const orig = await importOriginal<typeof import("../lib/models")>();
  return { ...orig, modelsCatalog: vi.fn(async () => []) };
});

// NotificationCenter 自带 client 依赖，与本用例无关
vi.mock("./NotificationCenter", () => ({ default: () => null }));

import StatusBar from "./StatusBar";
import { setActiveSessionId } from "../lib/state";

const flush = () => new Promise((r) => setTimeout(r, 0));

afterEach(() => {
  document.body.innerHTML = "";
  h.statusline.mockClear();
  setActiveSessionId("");
});

describe("StatusBar 会话切换即时刷新", () => {
  it("切换 activeSessionId 立即用新 id 重拉 statusline，不等 3s 轮询", async () => {
    setActiveSessionId("s1");
    const dispose = render(() => <StatusBar />, document.body);
    await flush();
    expect(h.statusline).toHaveBeenCalledTimes(1);
    expect(h.statusline).toHaveBeenLastCalledWith("s1");
    setActiveSessionId("s2");
    await flush();
    expect(h.statusline).toHaveBeenCalledTimes(2);
    expect(h.statusline).toHaveBeenLastCalledWith("s2");
    dispose();
  });
});
