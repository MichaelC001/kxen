// 重连订阅恢复实测（P1-15）：先快照再重开，open 回写同一 Map 也不得持续 reopen。
import { describe, expect, it } from "vitest";
import { restoreSubscriptions } from "./client";

describe("restoreSubscriptions", () => {
  it("open 回写新 key 时恰好恢复原有订阅，不形成 reopen 循环", async () => {
    const subs = new Map<string, string[]>([
      ["sub-old-1", ["llm.delta"]],
      ["sub-old-2", ["session:s1", "llm.delta"]],
    ]);
    const opened: string[][] = [];
    let n = 0;
    // 模拟 openSubscription：成功并回写新 streamId（旧实现的 Map 迭代会访问到这些新 entry = 死循环根因）
    await restoreSubscriptions(subs, (topics) => {
      opened.push(topics);
      subs.set(`sub-new-${n++}`, topics);
      return Promise.resolve();
    });
    expect(opened).toEqual([["llm.delta"], ["session:s1", "llm.delta"]]);
    expect([...subs.keys()]).toEqual(["sub-new-0", "sub-new-1"]);
  });

  it("单个重开失败不中断其余订阅恢复", async () => {
    const subs = new Map<string, string[]>([
      ["sub-1", ["a"]],
      ["sub-2", ["b"]],
    ]);
    const opened: string[][] = [];
    await restoreSubscriptions(subs, (topics) => {
      opened.push(topics);
      if (topics[0] === "a") return Promise.reject(new Error("boom"));
      subs.set("sub-new", topics);
      return Promise.resolve();
    });
    expect(opened).toEqual([["a"], ["b"]]);
    expect([...subs.keys()]).toEqual(["sub-new"]);
  });
});
