// 只写 localStorage；DOM 应用由 BaseLayout 预绘脚本负责，避免 FOUC 与跨页不同步。
import { mount } from "@cloudflare/nimbus-docs/client";

declare global {
  interface Window {
    __nbApplyTheme?: () => void;
  }
}

function initThemeToggle(button: HTMLElement): () => void {
  function handleClick() {
    const isDark = document.documentElement.getAttribute("data-mode") === "dark";
    try {
      localStorage.setItem("ui-mode", isDark ? "light" : "dark");
    } catch {
      // private mode 等忽略写失败。
    }
    window.__nbApplyTheme?.();
  }

  window.__nbApplyTheme?.();
  button.addEventListener("click", handleClick);
  return () => button.removeEventListener("click", handleClick);
}

mount("[data-nb-theme-toggle]", initThemeToggle);
