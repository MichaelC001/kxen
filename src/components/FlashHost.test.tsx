// FlashHost 行为测试：成功/失败 toast 渲染与关闭、动作 toast 执行并关闭（350 行门禁拆出）。
import { render } from "solid-js/web";
import { afterEach, describe, expect, it, vi } from "vitest";
import FlashHost from "./FlashHost";
import { flash, flashAction } from "../lib/flash";

afterEach(() => {
  document.body.innerHTML = "";
  for (const message of flash.msgs()) flash.dismiss(message.id);
});

describe("FlashHost", () => {
  it("渲染成功和失败消息并允许关闭", async () => {
    const dispose = render(() => <FlashHost />, document.body);
    flash.show("saved", "ok", 0);
    flash.show("failed", "err", 0);
    await vi.waitFor(() =>
      expect(document.body.querySelectorAll("button[title=关闭]")).toHaveLength(2),
    );
    expect(document.body.textContent).toContain("saved");
    expect(document.body.textContent).toContain("failed");
    // 点第一条的关闭按钮（文本不再是按钮本体，动作 toast 需要独立动作键）
    document.body.querySelector<HTMLButtonElement>("button[title=关闭]")!.click();
    expect(document.body.textContent).not.toContain("saved");
    expect(document.body.textContent).toContain("failed");
    dispose();
  });

  it("动作 toast：点动作键执行并关闭本条", async () => {
    const dispose = render(() => <FlashHost />, document.body);
    const run = vi.fn();
    flashAction("已回退", "撤销", run, 0);
    await vi.waitFor(() => expect(document.body.textContent).toContain("已回退"));
    const button = [...document.body.querySelectorAll<HTMLButtonElement>("button")].find((item) =>
      item.textContent?.includes("撤销"),
    );
    if (!button) throw new Error("button not found: 撤销");
    button.click();
    expect(run).toHaveBeenCalledTimes(1);
    expect(document.body.textContent).not.toContain("已回退");
    dispose();
  });
});
