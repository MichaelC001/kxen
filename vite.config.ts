import { defineConfig } from "vite-plus";
import solid from "vite-plugin-solid";
import tailwindcss from "@tailwindcss/vite";
import { playwright } from "@vitest/browser-playwright";

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
  // browser mode 下页面中途 reload 直接 flaky（vitest 报 "unexpectedly reloaded a test"）
  optimizeDeps: {
    include: [
      "shiki/core",
      "shiki/engine/oniguruma",
      "shiki/wasm",
      "shiki/themes/github-dark.mjs",
      "shiki/themes/github-light.mjs",
      "shiki/langs/rust.mjs",
      "shiki/langs/typescript.mjs",
      "shiki/langs/tsx.mjs",
      "shiki/langs/javascript.mjs",
      "shiki/langs/json.mjs",
      "shiki/langs/toml.mjs",
      "shiki/langs/bash.mjs",
      "shiki/langs/zsh.mjs",
      "shiki/langs/shell.mjs",
      "shiki/langs/python.mjs",
      "shiki/langs/markdown.mjs",
      "shiki/langs/yaml.mjs",
      "shiki/langs/html.mjs",
      "shiki/langs/css.mjs",
      "shiki/langs/diff.mjs",
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
