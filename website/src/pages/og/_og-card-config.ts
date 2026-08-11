/** 构建期 OG 卡片视觉配置。前导 `_` 让 Astro 跳过路由，便于与消费者同目录。 */

import type { OGImageOptions } from "astro-og-canvas";

export const ogCardConfig = {
  bgGradient: [
    [11, 11, 12],
    [26, 26, 28],
  ],
  border: { color: [39, 39, 42], width: 2, side: "inline-start" },
  padding: 96,
  logo: { path: "./public/icon.png", size: [72, 72] },
  fonts: ["./public/fonts/Inter-Bold.ttf", "./src/assets/fonts/NotoSansSC-Variable.ttf"],
  font: {
    title: {
      color: [250, 250, 250],
      size: 64,
      weight: "Bold",
      families: ["Inter", "Noto Sans SC"],
      lineHeight: 1.1,
    },
    description: {
      color: [161, 161, 170],
      size: 32,
      weight: "Bold",
      families: ["Inter", "Noto Sans SC"],
      lineHeight: 1.3,
    },
  },
  format: "PNG",
} satisfies Partial<OGImageOptions>;
