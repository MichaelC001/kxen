// 主题管理：light/dark，localStorage 持久化，默认跟随系统。
import { createSignal } from "solid-js";

export type Theme = "dark" | "light";

const KEY = "kxen-theme";

function initial(): Theme {
  const stored = localStorage.getItem(KEY);
  if (stored === "dark" || stored === "light") return stored;
  return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

export const [theme, setThemeSignal] = createSignal<Theme>(initial());

export function applyTheme(t: Theme): void {
  document.documentElement.dataset.theme = t;
  localStorage.setItem(KEY, t);
  setThemeSignal(t);
}

export function toggleTheme(): void {
  applyTheme(theme() === "dark" ? "light" : "dark");
}

/** 首帧前调用，避免暗->明闪屏。 */
export function initTheme(): void {
  document.documentElement.dataset.theme = theme();
}
