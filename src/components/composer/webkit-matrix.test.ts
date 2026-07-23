// Playwright-WebKit 键盘能力对照：textarea / 裸 contenteditable 两层（探针实证 input 事件原生正常）。
import { describe, expect, it } from "vitest";
import { userEvent } from "@vitest/browser/context";

describe("webkit keyboard matrix", () => {
  it("纯 textarea 可键入", async () => {
    const el = document.createElement("textarea");
    document.body.appendChild(el);
    el.focus();
    await userEvent.keyboard("ab");
    expect(el.value).toBe("ab");
    el.remove();
  });

  it("裸 contenteditable（带初始段落）可键入", async () => {
    const el = document.createElement("div");
    el.contentEditable = "true";
    const p = document.createElement("p");
    p.appendChild(document.createElement("br"));
    el.appendChild(p);
    document.body.appendChild(el);
    const seen: string[] = [];
    for (const t of ["keydown", "beforeinput", "input"]) el.addEventListener(t, () => seen.push(t));
    el.focus();
    await userEvent.keyboard("ab");
    await new Promise((r) => setTimeout(r, 100));
    console.log("[matrix] ce events:", seen.join(","), "text=", JSON.stringify(el.textContent));
    expect(el.textContent).toContain("ab");
    el.remove();
  });
});
