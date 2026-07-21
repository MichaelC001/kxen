import { createEffect } from "solid-js";
import { renderMarkdown, renderMermaid } from "../lib/markdown";
import { theme } from "../lib/theme";

/** Markdown 渲染组件：shiki 高亮 + mermaid 图表 + 代码块复制（事件委托）。 */
export default function Markdown(props: { text: string }) {
  let el: HTMLDivElement | undefined;

  const onClick = (e: MouseEvent) => {
    const btn = (e.target as HTMLElement).closest<HTMLButtonElement>(".code-copy");
    if (!btn || !el) return;
    const block = btn.closest(".code-block");
    const code = block?.querySelector("pre code")?.textContent ?? "";
    void navigator.clipboard.writeText(code).then(() => {
      btn.textContent = "已复制";
      setTimeout(() => (btn.textContent = "复制"), 1200);
    });
  };

  createEffect(() => {
    theme(); // 主题切换触发重渲染（shiki/mermaid 主题跟随）
    const html = renderMarkdown(props.text);
    if (!el) return;
    el.innerHTML = html;
    void renderMermaid(el);
  });

  return <div ref={(node) => (el = node)} class="md" onClick={onClick} />;
}
