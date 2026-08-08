// 主题管理：auto（跟随系统）/dark/light 三态，matchMedia 实时跟随，View Transition 圆形展开。
import { createSignal } from "solid-js";

export type Theme = "dark" | "light";
export type ThemeMode = "auto" | "dark" | "light";

const MODE_KEY = "kxen-theme-mode";
const EASE_OUT = "cubic-bezier(0.23, 1, 0.32, 1)";
const media = window.matchMedia("(prefers-color-scheme: light)");

function systemTheme(): Theme {
  return media.matches ? "light" : "dark";
}

function initialMode(): ThemeMode {
  try {
    const m = localStorage.getItem(MODE_KEY);
    return m === "dark" || m === "light" ? m : "auto";
  } catch {
    // storage 被禁用（隐私模式等）：按默认 auto 运行
    return "auto";
  }
}

export const [mode, setModeSignal] = createSignal<ThemeMode>(initialMode());
export const [theme, setThemeSignal] = createSignal<Theme>(current());

function current(): Theme {
  const m = mode();
  return m === "auto" ? systemTheme() : m;
}

function applyCurrent(): void {
  document.documentElement.dataset.theme = current();
  setThemeSignal(current());
}

/** 三态设置（auto/dark/light）。 */
export function setMode(m: ThemeMode): void {
  try {
    localStorage.setItem(MODE_KEY, m);
  } catch {
    // 隐私模式等写不进去：主题仅在本次会话内生效
  }
  setModeSignal(m);
  applyCurrent();
}

// 系统主题变化时 auto 模式实时跟随
media.addEventListener("change", () => {
  if (mode() === "auto") applyCurrent();
});

/** 手动指定明暗（脱离 auto）。 */
export function applyTheme(t: Theme): void {
  setMode(t);
}

interface ViewTransition {
  ready: Promise<void>;
}

/** 切换主题：支持 View Transition 时从点击处圆形展开，否则瞬时切换。 */
export function toggleTheme(x?: number, y?: number): void {
  const next: Theme = theme() === "dark" ? "light" : "dark";
  const start = (
    document as Document & { startViewTransition?: (cb: () => void) => ViewTransition }
  ).startViewTransition;
  const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (typeof start !== "function" || reduced) {
    setMode(next);
    return;
  }
  const transition = start.call(document, () => setMode(next));
  transition.ready
    .then(() => {
      const cx = x ?? window.innerWidth / 2;
      const cy = y ?? window.innerHeight / 2;
      const radius = Math.hypot(
        Math.max(cx, window.innerWidth - cx),
        Math.max(cy, window.innerHeight - cy),
      );
      document.documentElement.animate(
        { clipPath: [`circle(0px at ${cx}px ${cy}px)`, `circle(${radius}px at ${cx}px ${cy}px)`] },
        {
          duration: 280,
          easing: EASE_OUT,
          pseudoElement: "::view-transition-new(root)",
        } as KeyframeAnimationOptions,
      );
    })
    .catch(() => {}); // webkit 不支持 view-transition API：静默退回无动画切换
}

/** 首帧前调用，避免暗->明闪屏。 */
export function initTheme(): void {
  document.documentElement.dataset.theme = current();
}
