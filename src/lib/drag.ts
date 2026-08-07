// 窗口拖动：属性式 data-tauri-drag-region 在部分运行时失灵，显式 API 兜底（左键空白处即拖）。
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "./runtime";

export function onDragStart(e: MouseEvent): void {
  // 浏览器没有可拖的原生窗口：startDragging 会抛，直接不响应
  if (!isTauri()) return;
  if (e.button !== 0) return;
  // 交互子元素（按钮/链接/输入）不抢
  const t = e.target as HTMLElement;
  if (t.closest("button, a, input, select, textarea, [contenteditable]")) return;
  void getCurrentWindow().startDragging();
}
