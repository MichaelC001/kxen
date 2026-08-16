// 工具行路径跳转：web 模式（测试环境）复制路径兜底并明说；桌面端走 tauri opener（此处桩掉）。
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  openPath: vi.fn(async () => {}),
  writeClipboard: vi.fn(),
  flashOk: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: mocks.openPath }));
vi.mock("./clipboard", () => ({ writeClipboard: mocks.writeClipboard }));
vi.mock("./flash", () => ({ flashOk: mocks.flashOk, flashErr: vi.fn() }));
// test-setup 全局桩了 __TAURI_INTERNALS__：这里显式按 web 模式测降级分支
vi.mock("./runtime", () => ({ isTauri: () => false, isWeb: () => true }));

import { openToolPath } from "./file-open";

beforeEach(() => {
  vi.clearAllMocks();
});

describe("openToolPath web 兜底", () => {
  it("web 模式（无 __TAURI_INTERNALS__）复制路径并提示降级，不假装打开", async () => {
    await openToolPath("src/a.ts");
    expect(mocks.writeClipboard).toHaveBeenCalledWith("src/a.ts");
    expect(mocks.flashOk).toHaveBeenCalledOnce();
    expect(mocks.openPath).not.toHaveBeenCalled();
  });
});
