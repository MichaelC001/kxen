// 主题管理：light/dark，localStorage 持久化，默认跟随系统，View Transition 圆形展开。
import { createSignal } from "solid-js";

export type Theme = "dark" | "light";

const KEY = "kxen-theme";
const EASE_OUT = "cubic-bezier(0.23, 1, 0.32, 1)";

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
    applyTheme(next);
    return;
  }
  const transition = start.call(document, () => applyTheme(next));
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
    .catch(() => {});
}

/** 首帧前调用，避免暗->明闪屏。 */
export function initTheme(): void {
  document.documentElement.dataset.theme = theme();
}
