import { createEffect } from "solid-js";
import { renderMarkdown, renderMermaid } from "../lib/markdown";
import { theme } from "../lib/theme";

/** Markdown 渲染组件：shiki 高亮 + mermaid 图表（占位后实例化），跟随主题重渲染。 */
export default function Markdown(props: { text: string }) {
  let el: HTMLDivElement | undefined;
  createEffect(() => {
    theme(); // 主题切换触发重渲染（shiki/mermaid 主题跟随）
    const html = renderMarkdown(props.text);
    if (!el) return;
    el.innerHTML = html;
    void renderMermaid(el);
  });
  return <div ref={(node) => (el = node)} class="md" />;
}
