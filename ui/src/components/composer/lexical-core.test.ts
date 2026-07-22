// Lexical 内核在真实 WebKit 的键入冒烟（历史上「contenteditable 有了但键入不出字」的判官测试）。
// Lexical 内核在真实 WebKit 的键入冒烟（历史上「contenteditable 有了但键入不出字」的判官测试）。
import { describe, expect, it } from "vitest";
import { userEvent } from "@vitest/browser/context";
import { $getSelection } from "lexical";
import { mountComposer } from "./lexical-core";

describe("lexical core (webkit)", () => {
  it("setText/getText 回路 + insertChip 可见", () => {
    const el = document.createElement("div");
    document.body.appendChild(el);
    const core = mountComposer(el);
    core.setText("abc");
    expect(core.getText()).toBe("abc");
    core.insertChip({ kind: "file", ref: "/tmp/a.ts", label: "a.ts" });
    expect(core.getText()).toContain("a.ts");
    expect(core.extractChips()).toHaveLength(1);
    el.remove();
  });

  it("真实键入产出文本（WebKit 空 contenteditable 需先渲染初始段落）", async () => {
    const el = document.createElement("div");
    document.body.appendChild(el);
    const core = mountComposer(el);
    // WebKit 空 contenteditable 不建立 caret（beforeinput 有 input 无）：先离散渲染初始段落
    core.setText("");
    console.log("[probe] initial render:", el.innerHTML.slice(0, 80));
    await userEvent.click(el);
    const hasSel = core.editor.getEditorState().read(() => $getSelection() !== null);
    expect(hasSel).toBe(true);
    await userEvent.keyboard("hello kxen");
    await new Promise((r) => setTimeout(r, 150));
    expect(core.getText()).toBe("hello kxen");
    el.remove();
  });

  it("token chip 整块删除", async () => {
    const el = document.createElement("div");
    document.body.appendChild(el);
    const core = mountComposer(el);
    core.setText("");
    core.insertChip({ kind: "file", ref: "/tmp/a.ts", label: "a.ts" });
    expect(core.extractChips()).toHaveLength(1);
    el.focus();
    await userEvent.keyboard("{Home}{Delete}");
    await new Promise((r) => setTimeout(r, 100));
    expect(core.extractChips()).toHaveLength(0);
    el.remove();
  });
});
