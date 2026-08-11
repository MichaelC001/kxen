import type { HeadElement } from "@cloudflare/nimbus-docs/types";

// JSON-LD 经 NimbusHead set:html 注入；`<` 转 `\u003c`，避免内容里的 `</script>` 提前闭标签。
export function jsonLd(data: Record<string, unknown>): HeadElement {
  return {
    tag: "script",
    attrs: { type: "application/ld+json" },
    content: JSON.stringify(data).replace(/</g, "\\u003c"),
  };
}
