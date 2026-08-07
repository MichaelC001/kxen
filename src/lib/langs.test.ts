// shiki 语言清单单一来源守护：markdown 高亮白名单必须直接复用 langs.ts（同一引用），
// vite.config.ts 的 optimizeDeps 由同一清单派生（config 在 node 侧，测试跑在 browser，不同进程直读，
// 由 config 里的 import 与注释约束 + 本测试守护 markdown 这半边）。
import { describe, expect, it } from "vitest";
import { SHIKI_LANGS } from "./langs";
import { SHIKI_LANGS as MD_LANGS } from "./markdown";

describe("shiki 语言清单单一来源", () => {
  it("markdown.ts 复用同一份清单（同一引用，非拷贝）", () => {
    expect(MD_LANGS).toBe(SHIKI_LANGS);
  });

  it("清单无重复且都能映射到 shiki 子路径", () => {
    expect(SHIKI_LANGS.length).toBeGreaterThan(0);
    expect(new Set(SHIKI_LANGS).size).toBe(SHIKI_LANGS.length);
    for (const l of SHIKI_LANGS) {
      expect(`shiki/langs/${l}.mjs`).toMatch(/^shiki\/langs\/[a-z0-9-]+\.mjs$/);
    }
  });
});
