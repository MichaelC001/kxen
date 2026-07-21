// Markdown 渲染管线：marked + shiki（代码高亮）+ mermaid（图表）。
// 设计约束：mermaid 只渲染闭合代码块（流式中途的半成品块保持纯文本，不闪图）。
import { marked } from "marked";
import markedShiki from "marked-shiki";
import { createHighlighter, type Highlighter } from "shiki";

const LANGS = [
  "rust",
  "typescript",
  "tsx",
  "javascript",
  "json",
  "toml",
  "bash",
  "zsh",
  "shell",
  "python",
  "markdown",
  "yaml",
  "html",
  "css",
  "diff",
];

let highlighter: Highlighter | null = null;
let ready = false;

const MERMAID_BLOCK = /```mermaid\s*\n([\s\S]*?)```/g;

function escapeHtml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/** 代码块包装：语言标签 + 复制按钮（复制行为由 Markdown 组件事件委托实现）。 */
function wrapCodeBlock(body: string, lang: string): string {
  return `<div class="code-block" data-lang="${lang}"><div class="code-header"><span>${lang || "text"}</span><button class="code-copy" type="button">复制</button></div>${body}</div>`;
}

export async function initMarkdown(): Promise<void> {
  if (ready) return;
  highlighter = await createHighlighter({ themes: ["github-dark", "github-light"], langs: LANGS });
  marked.use(
    markedShiki({
      highlight(code, lang) {
        if (!highlighter || !lang || !LANGS.includes(lang)) {
          return wrapCodeBlock(`<pre><code>${escapeHtml(code)}</code></pre>`, lang || "text");
        }
        // 主题在渲染时动态读取（注册一次，不重复 marked.use）
        return wrapCodeBlock(highlighter.codeToHtml(code, { lang, theme: shikiTheme() }), lang);
      },
    }),
  );
  ready = true;
}

/** 当前主题对应的 shiki 主题名。 */
function shikiTheme(): string {
  return document.documentElement.dataset.theme === "light" ? "github-light" : "github-dark";
}

/** 渲染 markdown -> HTML（async：marked-shiki 扩展强制异步 parse）。mermaid 块先转占位 div，随后由 renderMermaid 实例化。 */
export async function renderMarkdown(text: string): Promise<string> {
  const withPlaceholders = text.replace(MERMAID_BLOCK, (_, source: string) => {
    return `\n\n<div class="mermaid">${escapeHtml(source.trim())}</div>\n\n`;
  });
  return (await marked.parse(withPlaceholders)) as string;
}

// mermaid 体积大（>500KB）：按需动态加载，首个 mermaid 块出现时才进内存
let mermaidLib: typeof import("mermaid").default | null = null;

async function ensureMermaid() {
  const theme = document.documentElement.dataset.theme === "light" ? "default" : "dark";
  if (!mermaidLib || mermaidTheme !== theme) {
    mermaidLib = (await import("mermaid")).default;
    mermaidLib.initialize({
      startOnLoad: false,
      theme,
      securityLevel: "strict",
      fontFamily: "system-ui, -apple-system, sans-serif",
    });
    mermaidTheme = theme;
  }
  return mermaidLib;
}

let mermaidTheme = "";

let mermaidSeq = 0;

/** 把容器里的 .mermaid 占位 div 渲染成 SVG（幂等：已渲染的跳过）。 */
export async function renderMermaid(container: HTMLElement): Promise<void> {
  if (!ready) return;
  const nodes = container.querySelectorAll<HTMLElement>(".mermaid:not([data-rendered])");
  if (nodes.length === 0) return;
  const mermaid = await ensureMermaid();
  for (const node of nodes) {
    const source = node.textContent ?? "";
    node.dataset.rendered = "pending";
    try {
      const { svg } = await mermaid.render(`kxen-mmd-${mermaidSeq++}`, source);
      node.innerHTML = svg;
      node.dataset.rendered = "done";
    } catch {
      node.dataset.rendered = "error";
      node.innerHTML = `<pre><code>${escapeHtml(source)}</code></pre>`;
    }
  }
}
