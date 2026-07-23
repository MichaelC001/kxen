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
  test: {
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
