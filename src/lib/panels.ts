// 三栏宽度：左 Sidebar 与右 dock 可拖拽调宽，localStorage 持久化，双击把手复位。
import { createSignal } from "solid-js";

interface PanelSpec {
  min: number;
  max: number;
  def: number;
  key: string;
}

export const SIDEBAR: PanelSpec = { min: 176, max: 420, def: 208, key: "kxen.sidebar.w" };
export const DOCK: PanelSpec = { min: 232, max: 520, def: 256, key: "kxen.dock.w" };

function clamp(spec: PanelSpec, n: number): number {
  return Math.min(spec.max, Math.max(spec.min, Math.round(n)));
}

function load(spec: PanelSpec): number {
  const raw = globalThis.localStorage?.getItem(spec.key);
  const n = raw === null || raw === undefined ? NaN : Number(raw);
  return Number.isFinite(n) ? clamp(spec, n) : spec.def;
}

function persist(spec: PanelSpec, n: number): void {
  try {
    globalThis.localStorage?.setItem(spec.key, String(n));
  } catch {
    // 隐私模式等写不进去：宽度仅在本次会话内生效
  }
}

export const [sidebarWidth, setSidebarWidth] = createSignal(load(SIDEBAR));
export const [dockWidth, setDockWidth] = createSignal(load(DOCK));

/** 拖拽增量（px，向右为正）；右栏由调用方取反传入。 */
export function adjustSidebar(dx: number): void {
  const w = clamp(SIDEBAR, sidebarWidth() + dx);
  setSidebarWidth(w);
  persist(SIDEBAR, w);
}

export function adjustDock(dx: number): void {
  const w = clamp(DOCK, dockWidth() + dx);
  setDockWidth(w);
  persist(DOCK, w);
}

export function resetSidebar(): void {
  setSidebarWidth(SIDEBAR.def);
  persist(SIDEBAR, SIDEBAR.def);
}

export function resetDock(): void {
  setDockWidth(DOCK.def);
  persist(DOCK, DOCK.def);
}
