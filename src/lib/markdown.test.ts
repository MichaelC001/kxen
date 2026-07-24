// Markdown sanitizer 实测（报告 P1-16）：marked 原样保留 raw HTML，
// 写 innerHTML 前必须过 DOMPurify——注入面清除 + 正常高亮结构保留。
import { describe, expect, it } from "vitest";
import { initMarkdown, renderMarkdown } from "./markdown";

describe("markdown sanitizer", () => {
  it("script 标签被清除", async () => {
    const html = await renderMarkdown("before\n\n<script>alert(1)</script>\n\nafter");
    expect(html).not.toContain("<script");
    expect(html).not.toContain("alert(1)");
    expect(html).toContain("before");
    expect(html).toContain("after");
  });

  it("事件属性（onerror）被清除", async () => {
    const html = await renderMarkdown('<img src="x" onerror="alert(1)">');
    expect(html).not.toContain("onerror");
    expect(html).toContain("<img");
  });

  it("javascript: URL 被清除", async () => {
    const html = await renderMarkdown('<a href="javascript:alert(1)">click</a>');
    expect(html).not.toContain("javascript:");
    expect(html).toContain("click");
  });

  it("style 属性被清除（防 CSS 覆盖 UI 钓鱼层）", async () => {
    const html = await renderMarkdown('<div style="position:fixed;inset:0">x</div>');
    expect(html).not.toContain("style=");
    expect(html).toContain("x");
  });

  it("style 标签连同内容被清除", async () => {
    const html = await renderMarkdown("<style>.md{display:none}</style>");
    expect(html).not.toContain("<style");
    expect(html).not.toContain("display:none");
  });

  it("mermaid 占位 div 经 sanitizer 后保留", async () => {
    const html = await renderMarkdown("```mermaid\ngraph TD; A-->B;\n```");
    expect(html).toContain('<div class="mermaid">');
  });

  it("正常 code block 的 class 与 shiki 内联 style 保留", async () => {
    await initMarkdown();
    const html = await renderMarkdown("```rust\nfn main() {}\n```");
    expect(html).toContain('class="code-block"');
    expect(html).toContain('data-lang="rust"');
    expect(html).toContain("code-copy");
    // shiki 高亮颜色在 .code-block 内部，属于 style 例外放行
    expect(html).toContain('class="shiki');
    expect(html).toContain("style=");
  });
});
