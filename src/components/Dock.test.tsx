// Dock resync 自愈：goal.update/task.update 丢帧后 topic 流不自愈，resync 信号按真源重拉 goal 与后台任务。
import { render } from "solid-js/web";
import { afterEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  goalFocus: vi.fn(async () => null),
  taskList: vi.fn(async () => [] as unknown[]),
  agentDiffStatus: vi.fn(async () => [] as unknown[]),
  onTopic: vi.fn(async (_topics: string[], _handler: unknown) => () => {}),
  resync: new Set<() => void>(),
}));

vi.mock("../lib/chat", async (importOriginal) => {
  // 全量 mock 会断 state.ts -> session-model 的 currentModel 绑定：铺开真实模块，只桩测试关注的 7 个
  const orig = await importOriginal<typeof import("../lib/chat")>();
  return {
    ...orig,
    goalFocus: h.goalFocus,
    goalTransit: vi.fn(async () => true),
    taskList: h.taskList,
    taskKill: vi.fn(async () => true),
    agentDiffStatus: h.agentDiffStatus,
    agentDiffFile: vi.fn(async () => ""),
    onTopic: h.onTopic,
  };
});

vi.mock("../lib/client", () => ({
  client: {
    onResync: (cb: () => void) => {
      h.resync.add(cb);
      return () => h.resync.delete(cb);
    },
  },
}));

// Markdown / DockWorktree 与本测试无关（重依赖 + 自带 RPC），桩掉保持用例聚焦
vi.mock("./Markdown", () => ({ default: () => null }));
vi.mock("./DockWorktree", () => ({ default: () => null }));

import Dock from "./Dock";

afterEach(() => {
  document.body.innerHTML = "";
  h.goalFocus.mockClear();
  h.taskList.mockClear();
  h.resync.clear();
});

describe("Dock resync 自愈", () => {
  it("resync 信号触发 goal/tasks 重拉，卸载后注销回调", async () => {
    const dispose = render(() => <Dock />, document.body);
    await new Promise((r) => setTimeout(r, 0));
    expect(h.goalFocus).toHaveBeenCalledTimes(1);
    expect(h.taskList).toHaveBeenCalledTimes(1);
    expect(h.resync.size).toBe(1);
    for (const cb of h.resync) cb();
    await new Promise((r) => setTimeout(r, 0));
    expect(h.goalFocus).toHaveBeenCalledTimes(2);
    expect(h.taskList).toHaveBeenCalledTimes(2);
    dispose();
    expect(h.resync.size).toBe(0);
  });
});
