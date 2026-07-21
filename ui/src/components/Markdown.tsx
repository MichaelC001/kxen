import { createEffect } from "solid-js";
import { renderMarkdown, renderMermaid } from "../lib/markdown";

/** Markdown 渲染组件：shiki 高亮 + mermaid 图表（占位后实例化）。 */
export default function Markdown(props: { text: string }) {
  let el: HTMLDivElement | undefined;
  createEffect(() => {
    const html = renderMarkdown(props.text);
    if (!el) return;
    el.innerHTML = html;
    void renderMermaid(el);
  });
  return <div ref={(node) => (el = node)} class="md" />;
}
