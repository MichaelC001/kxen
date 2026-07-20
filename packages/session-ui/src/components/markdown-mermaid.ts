import mermaid from "mermaid"

// mermaid 后处理：markdown 渲染完成后的 DOM 里把 mermaid 代码块替换为 SVG。
// 渲染失败的块保留原代码，不阻塞其余内容。

let initialized = false

function ensureInit() {
  if (initialized) return
  initialized = true
  const light = document.documentElement.dataset.theme?.includes("light") ?? false
  mermaid.initialize({
    startOnLoad: false,
    securityLevel: "strict",
    theme: light ? "default" : "dark",
  })
}

const SELECTOR = 'pre > code.language-mermaid, pre > code[data-language="mermaid"]'

export async function renderMermaidBlocks(container: HTMLElement) {
  const blocks = container.querySelectorAll<HTMLElement>(SELECTOR)
  if (blocks.length === 0) return
  ensureInit()
  let seq = 0
  for (const code of blocks) {
    const pre = code.closest("pre")
    if (!pre || pre.dataset.mermaidDone) continue
    pre.dataset.mermaidDone = "1"
    const source = code.textContent ?? ""
    try {
      const { svg } = await mermaid.render(`kxen-mermaid-${Date.now()}-${seq++}`, source)
      const wrapper = document.createElement("div")
      wrapper.className = "mermaid-diagram"
      wrapper.innerHTML = svg
      pre.replaceWith(wrapper)
    } catch {
      // 语法错误的图保留源码展示
      delete pre.dataset.mermaidDone
    }
  }
}
