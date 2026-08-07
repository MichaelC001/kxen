// runtime 环境判定：Tauri webview 注入 __TAURI_INTERNALS__（test-setup 桩同字段），纯浏览器没有。
import { afterEach, describe, expect, it } from "vitest";
import { isTauri, isWeb } from "./runtime";

const w = window as unknown as { __TAURI_INTERNALS__?: unknown };
let saved: unknown;

afterEach(() => {
  w.__TAURI_INTERNALS__ = saved;
});

describe("runtime", () => {
  it("有 __TAURI_INTERNALS__（webview / 测试桩）判 Tauri", () => {
    saved = w.__TAURI_INTERNALS__;
    expect(isTauri()).toBe(true);
    expect(isWeb()).toBe(false);
  });

  it("无 __TAURI_INTERNALS__（纯浏览器）判 web", () => {
    saved = w.__TAURI_INTERNALS__;
    delete w.__TAURI_INTERNALS__;
    expect(isTauri()).toBe(false);
    expect(isWeb()).toBe(true);
  });
});
