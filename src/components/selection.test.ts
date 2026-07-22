// 文本选择规则实测：交互元素禁选，内容区可选（A1）。
import "../styles.css";
import { describe, expect, it } from "vitest";

function selOf(el: HTMLElement): string {
  const cs = getComputedStyle(el);
  return cs.getPropertyValue("user-select") || cs.getPropertyValue("-webkit-user-select");
}

describe("user-select rules (webkit)", () => {
  it("交互元素 user-select: none", () => {
    const btn = document.createElement("button");
    btn.textContent = "点我";
    document.body.appendChild(btn);
    expect(selOf(btn)).toBe("none");
    btn.remove();
  });

  it("interactive 类及其子元素禁选", () => {
    const row = document.createElement("div");
    row.className = "interactive";
    const span = document.createElement("span");
    span.textContent = "会话标题";
    row.appendChild(span);
    document.body.appendChild(row);
    expect(selOf(row)).toBe("none");
    expect(selOf(span)).toBe("none");
    row.remove();
  });

  it("普通内容文本默认可选", () => {
    const p = document.createElement("p");
    p.textContent = "assistant 回复内容";
    document.body.appendChild(p);
    expect(selOf(p)).not.toBe("none");
    p.remove();
  });
});
