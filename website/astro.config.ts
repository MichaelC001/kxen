import { defineConfig } from "astro/config";
import icon from "astro-icon";
import tailwindcss from "@tailwindcss/vite";
import nimbus, { defineConfig as defineNimbusConfig } from "@cloudflare/nimbus-docs";
import { tableScroll } from "@cloudflare/nimbus-docs/markdown";

const nimbusConfig = defineNimbusConfig({
  site: "https://kxen.ai",
  title: "kxen",
  description: "面向复杂软件工程任务的 macOS 原生 Coding Agent Harness。",
  locale: "zh-CN",
  homeLabel: "首页",
  github: null,
  editPattern: null,
  socialImage: "/og.png",
  socialImageAlt: "kxen 产品官网与文档",
});

export default defineConfig({
  output: "static",
  // Tailwind v4 via its Vite plugin (the integration Astro recommends for
  // Tailwind v4 — replaces the PostCSS plugin, which doesn't build under
  // Astro 7's Vite 8 bundler).
  vite: {
    plugins: [tailwindcss()],
    resolve: {
      alias: [
        {
          find: /^vscode-jsonrpc\/lib\/common\/(?:events|cancellation)\.js$/,
          // Langium 3 imports the legacy subpath, but vscode-jsonrpc 9 only
          // exports the same public event and cancellation APIs from its root.
          replacement: "vscode-jsonrpc",
        },
      ],
    },
  },
  // Hover-prefetch link targets so full-page navigations feel instant without
  // a client-side router.
  prefetch: {
    prefetchAll: true,
    defaultStrategy: "hover",
  },
  integrations: [
    icon(),
    nimbus(nimbusConfig, {
      rules: {
        "nimbus/frontmatter-shape": "error",
        "nimbus/internal-link": "error",
        "nimbus/description-required": "error",
        "nimbus/single-h1": "error",
        "nimbus/heading-hierarchy": "error",
        "nimbus/code-block-lang": "error",
        "nimbus/duplicate-heading-text": "warn",
        "nimbus/bare-url": "warn",
      },
      markdown: {
        hastPlugins: [tableScroll()],
      },
    }),
  ],
});
