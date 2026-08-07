// shiki 语言清单唯一来源：markdown.ts（高亮白名单）与 vite.config.ts（optimizeDeps 预声明）都从这里取。
// 独立成零依赖文件：vite.config 在 node 侧加载，不能经 markdown.ts 带入 DOMPurify/marked 等浏览器依赖。
export const SHIKI_LANGS: readonly string[] = [
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
