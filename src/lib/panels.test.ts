// panels 栏宽：拖拽增量钳制在 min/max，localStorage 持久化，复位回默认宽。
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  adjustDock,
  adjustSidebar,
  DOCK,
  dockWidth,
  resetDock,
  resetSidebar,
  SIDEBAR,
  sidebarWidth,
} from "./panels";

beforeEach(() => {
  localStorage.clear();
  resetSidebar();
  resetDock();
});

describe("panels 栏宽", () => {
  it("拖拽增量累加并持久化", () => {
    adjustSidebar(50);
    expect(sidebarWidth()).toBe(SIDEBAR.def + 50);
    expect(localStorage.getItem(SIDEBAR.key)).toBe(String(SIDEBAR.def + 50));
  });

  it("钳制在 min/max 内", () => {
    adjustSidebar(-99999);
    expect(sidebarWidth()).toBe(SIDEBAR.min);
    adjustSidebar(99999);
    expect(sidebarWidth()).toBe(SIDEBAR.max);
    adjustDock(-99999);
    expect(dockWidth()).toBe(DOCK.min);
    adjustDock(99999);
    expect(dockWidth()).toBe(DOCK.max);
  });

  it("复位回默认宽并清掉持久化值", () => {
    adjustSidebar(100);
    resetSidebar();
    expect(sidebarWidth()).toBe(SIDEBAR.def);
    expect(localStorage.getItem(SIDEBAR.key)).toBe(String(SIDEBAR.def));
  });

  it("存储写入失败时仍更新当前会话宽度", () => {
    const setItem = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("quota");
    });
    expect(() => adjustDock(10)).not.toThrow();
    expect(dockWidth()).toBe(DOCK.def + 10);
    setItem.mockRestore();
  });
});
