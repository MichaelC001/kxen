// 文本选择规则实测：全局禁选，仅内容区（.selectable）与输入控件可选。
import "../styles.css";
import { describe, expect, it } from "vitest";

function selOf(el: HTMLElement): string {
  const cs = getComputedStyle(el);
  return cs.getPropertyValue("user-select") || cs.getPropertyValue("-webkit-user-select");
}

describe("user-select rules (webkit)", () => {
  it("普通元素默认禁选", () => {
    const p = document.createElement("p");
    p.textContent = "侧栏标签";
    document.body.appendChild(p);
    expect(selOf(p)).toBe("none");
    p.remove();
  });

  it("selectable 容器及子元素可选", () => {
    const box = document.createElement("div");
    box.className = "selectable";
    const span = document.createElement("span");
    span.textContent = "assistant 回复内容";
    box.appendChild(span);
    document.body.appendChild(box);
    expect(selOf(box)).toBe("text");
    expect(selOf(span)).toBe("text");
    box.remove();
  });

  it("输入控件可选", () => {
    const input = document.createElement("input");
    const ta = document.createElement("textarea");
    const ce = document.createElement("div");
    ce.contentEditable = "true";
    const ceChild = document.createElement("span");
    ce.appendChild(ceChild);
    document.body.append(input, ta, ce);
    expect(selOf(input)).toBe("text");
    expect(selOf(ta)).toBe("text");
    expect(selOf(ce)).toBe("text");
    expect(selOf(ceChild)).toBe("text");
    input.remove();
    ta.remove();
    ce.remove();
  });

  it("内容区内的按钮仍禁选", () => {
    const box = document.createElement("div");
    box.className = "selectable";
    const btn = document.createElement("button");
    btn.textContent = "复制";
    box.appendChild(btn);
    document.body.appendChild(box);
    expect(selOf(btn)).toBe("none");
    box.remove();
  });
});
