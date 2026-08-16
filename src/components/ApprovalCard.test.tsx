// ApprovalCard：tool_define 审批的 reason 是 markdown（描述 + 参数 Schema + 实现源码），
// 走 Markdown 渲染拿源码高亮；其余审批 reason 保持纯文本。
import { render } from "solid-js/web";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("./Markdown", () => ({
  default: (p: { text: string }) => <div data-md="">{p.text}</div>,
}));

import ApprovalCard from "./ApprovalCard";
import type { ApprovalItem } from "../lib/items";

const item = (command: string, reason: string): ApprovalItem => ({
  kind: "approval",
  approvalId: "ap1",
  command,
  reason,
});

afterEach(() => {
  document.body.innerHTML = "";
});

describe("ApprovalCard 动态工具注册审批", () => {
  it("tool_define 审批的 reason 走 Markdown 渲染（源码高亮）", () => {
    const reason = "注册动态工具 `dyn__echo_ab12cd34`。\n\n```js\nreturn 1;\n```";
    const dispose = render(
      () => (
        <ApprovalCard
          item={item("tool_define dyn__echo_ab12cd34", reason)}
          onRespond={async () => {}}
        />
      ),
      document.body,
    );
    expect(document.body.querySelector("[data-md]")?.textContent).toBe(reason);
    dispose();
  });

  it("tool_undefine 审批的 reason 同走 Markdown 渲染", () => {
    const reason = "卸载动态工具 `dyn__echo_ab12cd34`。\n\n**描述**：echo";
    const dispose = render(
      () => (
        <ApprovalCard
          item={item("tool_undefine dyn__echo_ab12cd34", reason)}
          onRespond={async () => {}}
        />
      ),
      document.body,
    );
    expect(document.body.querySelector("[data-md]")?.textContent).toBe(reason);
    dispose();
  });

  it("普通审批 reason 保持纯文本，不走 Markdown", () => {
    const dispose = render(
      () => <ApprovalCard item={item("rm -rf x", "危险 `rm`")} onRespond={async () => {}} />,
      document.body,
    );
    expect(document.body.querySelector("[data-md]")).toBeNull();
    expect(document.body.textContent).toContain("危险 `rm`");
    dispose();
  });
});
