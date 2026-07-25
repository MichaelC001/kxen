import { defineConfig } from "vite-plus";
import solid from "vite-plugin-solid";
import tailwindcss from "@tailwindcss/vite";
import { playwright } from "@vitest/browser-playwright";
import { SHIKI_LANGS } from "./src/lib/langs";

export default defineConfig({
  plugins: [solid(), tailwindcss()],
  server: {
    port: 7823,
    strictPort: true,
  },
  build: {
    target: "esnext",
    outDir: "dist",
  },
  // shiki 细粒度子路径依赖：不预声明会在 dev/test 首跑时触发 dep optimizer 二次扫描，
  // browser mode 下页面中途 reload 直接 flaky（vitest 报 "unexpectedly reloaded a test"）。
  // 语言子路径由 SHIKI_LANGS 派生（与 markdown.ts 高亮白名单同一份清单，勿再手工列第二遍）
  optimizeDeps: {
    include: [
      // TopAgentBar 经 lib/drag 引入：懒优化会中途 reload 测试页
      "@tauri-apps/api/window",
      "shiki/core",
      "shiki/engine/oniguruma",
      "shiki/wasm",
      "shiki/themes/github-dark.mjs",
      "shiki/themes/github-light.mjs",
      ...SHIKI_LANGS.map((l) => `shiki/langs/${l}.mjs`),
    ],
  },
  test: {
    // 显式 node：vite-plugin-solid 在 mode=test 且未设 environment 时注入 jsdom，
    // vitest 4 启动时对该 environment 做依赖检查，jsdom 未装则 exit 1（browser 测试实际不用它）
    environment: "node",
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    setupFiles: ["src/test-setup.ts"],
    browser: {
      enabled: true,
      provider: playwright(),
      headless: true,
      instances: [{ browser: "webkit" }],
    },
  },
});
