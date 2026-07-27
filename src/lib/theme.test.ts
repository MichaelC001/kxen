import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

type MediaListener = () => void;

const media = {
  light: false,
  reduced: false,
  listeners: [] as MediaListener[],
};

beforeEach(() => {
  vi.resetModules();
  localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
  media.light = false;
  media.reduced = false;
  media.listeners = [];
  vi.stubGlobal(
    "matchMedia",
    vi.fn((query: string) => {
      const reduced = query.includes("prefers-reduced-motion");
      return {
        get matches() {
          return reduced ? media.reduced : media.light;
        },
        addEventListener: (_event: string, listener: MediaListener) =>
          media.listeners.push(listener),
        removeEventListener: vi.fn(),
      };
    }),
  );
});

afterEach(() => {
  vi.unstubAllGlobals();
  Reflect.deleteProperty(document, "startViewTransition");
  vi.restoreAllMocks();
});

describe("theme", () => {
  it("initializes auto mode, follows system changes, and persists explicit mode", async () => {
    media.light = true;
    const theme = await import("./theme");
    expect(theme.mode()).toBe("auto");
    expect(theme.theme()).toBe("light");

    theme.initTheme();
    expect(document.documentElement.dataset.theme).toBe("light");

    media.light = false;
    media.listeners[0]?.();
    expect(theme.theme()).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");

    theme.applyTheme("light");
    expect(theme.mode()).toBe("light");
    expect(localStorage.getItem("kxen-theme-mode")).toBe("light");
    media.light = false;
    media.listeners[0]?.();
    expect(theme.theme()).toBe("light");
  });

  it("falls back to immediate toggle without transition support or with reduced motion", async () => {
    localStorage.setItem("kxen-theme-mode", "invalid");
    const theme = await import("./theme");
    theme.toggleTheme();
    expect(theme.theme()).toBe("light");

    const start = vi.fn();
    Object.defineProperty(document, "startViewTransition", {
      configurable: true,
      value: start,
    });
    media.reduced = true;
    theme.toggleTheme(10, 20);
    expect(theme.theme()).toBe("dark");
    expect(start).not.toHaveBeenCalled();
  });

  it("animates a view transition from explicit and default origins", async () => {
    const ready = Promise.resolve();
    const start = vi.fn((callback: () => void) => {
      callback();
      return { ready };
    });
    const animate = vi.fn();
    Object.defineProperty(document, "startViewTransition", {
      configurable: true,
      value: start,
    });
    Object.defineProperty(document.documentElement, "animate", {
      configurable: true,
      value: animate,
    });
    const theme = await import("./theme");

    theme.toggleTheme(10, 20);
    await ready;
    await Promise.resolve();
    expect(start).toHaveBeenCalledOnce();
    expect(animate).toHaveBeenCalledWith(
      expect.objectContaining({
        clipPath: expect.arrayContaining([expect.stringContaining("10px 20px")]),
      }),
      expect.objectContaining({ duration: 280 }),
    );

    animate.mockClear();
    theme.toggleTheme();
    await ready;
    await Promise.resolve();
    expect(animate).toHaveBeenCalledOnce();
  });

  it("absorbs transition readiness rejection", async () => {
    const start = vi.fn((callback: () => void) => {
      callback();
      return { ready: Promise.reject(new Error("unsupported")) };
    });
    Object.defineProperty(document, "startViewTransition", {
      configurable: true,
      value: start,
    });
    const theme = await import("./theme");
    theme.toggleTheme();
    await Promise.resolve();
    await Promise.resolve();
    expect(theme.theme()).toBe("light");
  });
});
