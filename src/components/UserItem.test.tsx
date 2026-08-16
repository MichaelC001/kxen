// UserItem 注入类消息：来源消息（teammate/task notification）与 @/# 上下文引用默认折叠，
// 折叠标题写明来源；普通用户口信不受影响。
import { render } from "solid-js/web";
import { afterEach, describe, expect, it } from "vitest";
import UserItem from "./UserItem";
import type { MsgItem } from "../lib/items";

function item(partial: Partial<MsgItem>): MsgItem {
  return { kind: "msg", role: "user", content: "", messageId: "m1", ...partial };
}

function mount(entry: MsgItem) {
  return render(
    () => (
      <UserItem
        item={entry}
        sessionId={() => "s1"}
        onFork={() => {}}
        onEditResend={async () => true}
        onRewind={() => {}}
        onRetry={() => {}}
        retrying={() => false}
      />
    ),
    document.body,
  );
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("UserItem 注入类消息默认折叠", () => {
  it("task notification：折叠卡标题 = 来源 + 首行摘要，正文不进气泡", () => {
    const dispose = mount(
      item({
        content: "[task notification] agent builder (execution) finished:\n修复完成，共改 3 个文件",
        source: "task notification",
      }),
    );
    const card = document.body.querySelector("details[data-testid='injected-msg']")!;
    expect(card).toBeTruthy();
    expect(card.hasAttribute("open")).toBe(false);
    expect(card.querySelector("summary")!.textContent).toContain("task notification");
    expect(card.querySelector("summary")!.textContent).toContain(
      "agent builder (execution) finished:",
    );
    // 全文在折叠体内，不以 accent 气泡直出
    expect(document.body.querySelector(".rounded-2xl")).toBeNull();
    card.querySelector("summary")!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(card.hasAttribute("open")).toBe(true);
    expect(card.textContent).toContain("修复完成，共改 3 个文件");
    dispose();
  });

  it("teammate 来信：标题剥离前缀只留正文首行", () => {
    const dispose = mount(
      item({ content: "[teammate worker] 我已完成分配", source: "teammate worker" }),
    );
    const summary = document.body.querySelector("[data-testid='injected-msg'] summary")!;
    expect(summary.textContent).toContain("teammate worker");
    expect(summary.textContent).toContain("我已完成分配");
    expect(summary.textContent).not.toContain("[teammate worker]");
    dispose();
  });

  it("普通用户口信保持气泡直出，不出现折叠卡", () => {
    const dispose = mount(item({ content: "帮我改 bug" }));
    expect(document.body.querySelector("[data-testid='injected-msg']")).toBeNull();
    expect(document.body.textContent).toContain("帮我改 bug");
    dispose();
  });

  it("上下文引用默认折叠：标题列出引用来源，展开见 note 全文", () => {
    const dispose = mount(
      item({
        content: "看下这个文件",
        context: [
          { type: "file", path: "src/main.ts" },
          { type: "note", text: "记得跑测试" },
        ],
      }),
    );
    const card = document.body.querySelector("details[data-testid='injected-context']")!;
    // details 闭合时正文在 DOM 但不渲染（浏览器原生语义），折叠态用 open 属性断言
    expect(card.hasAttribute("open")).toBe(false);
    expect(card.querySelector("summary")!.textContent).toContain("file src/main.ts");
    expect(card.querySelector("summary")!.textContent).not.toContain("记得跑测试");
    card.querySelector("summary")!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(card.textContent).toContain("记得跑测试");
    // 用户正文气泡不受影响
    expect(document.body.textContent).toContain("看下这个文件");
    dispose();
  });
});
