import DOMPurify from "dompurify";

let diagrams: HTMLPreElement[] = [];
const captured = new WeakSet<HTMLPreElement>();
let dialog: HTMLDialogElement | null = null;

function uniqueMermaidId(): string {
  const random =
    globalThis.crypto?.randomUUID?.() ??
    `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
  return `mermaid-${random}`;
}

function captureDiagramSource(diagram: HTMLPreElement): void {
  if (captured.has(diagram)) return;
  diagram.setAttribute("data-diagram", diagram.textContent as string);
  captured.add(diagram);
}

function showRenderError(diagram: HTMLPreElement): void {
  captureDiagramSource(diagram);
  diagram.textContent = "图表加载失败。";
  diagram.setAttribute("data-error", "true");
  diagram.setAttribute("data-processed", "true");
}

function getDialog(): HTMLDialogElement {
  if (dialog) return dialog;

  dialog = document.createElement("dialog");
  dialog.className = "mermaid-dialog";
  dialog.innerHTML = `
    <div class="mermaid-dialog-body"></div>
    <button class="mermaid-dialog-close" aria-label="关闭图表">
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <line x1="18" y1="6" x2="6" y2="18"></line>
        <line x1="6" y1="6" x2="18" y2="18"></line>
      </svg>
    </button>
  `;
  document.body.appendChild(dialog);

  function closeWithAnimation(): void {
    if (!dialog || !dialog.open) return;
    dialog.classList.add("closing");
    dialog.addEventListener(
      "animationend",
      () => {
        dialog?.classList.remove("closing");
        dialog?.close();
        document.documentElement.style.overflow = "";
      },
      { once: true },
    );
  }

  dialog.addEventListener("click", (event) => {
    if (event.target === dialog) closeWithAnimation();
  });
  dialog.querySelector(".mermaid-dialog-close")?.addEventListener("click", closeWithAnimation);
  dialog.addEventListener("cancel", (event) => {
    event.preventDefault();
    closeWithAnimation();
  });

  return dialog;
}

function openDiagram(container: HTMLElement): void {
  const currentDialog = getDialog();
  const clone = container.cloneNode(true) as HTMLElement;
  clone.querySelector(".mermaid-expand")?.remove();

  const svg = clone.querySelector("svg");
  if (svg) {
    svg.removeAttribute("style");
    svg.setAttribute("width", "100%");
    svg.setAttribute("height", "auto");
  }

  const body = currentDialog.querySelector(".mermaid-dialog-body");
  if (!body) return;
  body.replaceChildren(clone);

  clone.addEventListener("click", (event) => {
    const target = event.target as Element;
    if (target.closest("a") || target.closest(".clickable")) {
      currentDialog.close();
      document.documentElement.style.overflow = "";
    }
  });

  document.documentElement.style.overflow = "hidden";
  currentDialog.showModal();
}

function getFontFamily(): string {
  const computedStyle = getComputedStyle(document.documentElement);
  const font = computedStyle.getPropertyValue("--nb-font-sans").trim();
  return font || "system-ui, -apple-system, sans-serif";
}

function getPageBackground(): string {
  const style = getComputedStyle(document.documentElement);
  const background = style.getPropertyValue("--nb-background").trim();
  if (isMermaidSupportedColor(background)) return background;
  return document.documentElement.getAttribute("data-mode") === "dark" ? "#0f0f0f" : "#ffffff";
}

function getThemeColor(name: string, fallback: string): string {
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return isMermaidSupportedColor(value) ? value : fallback;
}

function isMermaidSupportedColor(value: string): boolean {
  return (
    /^#(?:[0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(value) ||
    /^rgba?\(\s*\d+(?:\.\d+)?%?\s*,\s*\d+(?:\.\d+)?%?\s*,\s*\d+(?:\.\d+)?%?(?:\s*,\s*(?:0|1|0?\.\d+|\d+(?:\.\d+)?%))?\s*\)$/i.test(
      value,
    ) ||
    /^hsla?\(\s*-?\d+(?:\.\d+)?(?:deg|rad|turn)?\s*,\s*\d+(?:\.\d+)?%\s*,\s*\d+(?:\.\d+)?%(?:\s*,\s*(?:0|1|0?\.\d+|\d+(?:\.\d+)?%))?\s*\)$/i.test(
      value,
    )
  );
}

function wrapDiagram(diagram: HTMLPreElement): void {
  if (diagram.parentElement?.classList.contains("mermaid-container")) return;

  const container = document.createElement("div");
  container.className = "mermaid-container";
  diagram.parentNode?.insertBefore(container, diagram);
  container.appendChild(diagram);

  const expandButton = document.createElement("button");
  expandButton.className = "mermaid-expand";
  expandButton.setAttribute("aria-label", "展开图表");
  expandButton.innerHTML = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
    <polyline points="15 3 21 3 21 9"></polyline>
    <polyline points="9 21 3 21 3 15"></polyline>
    <line x1="21" y1="3" x2="14" y2="10"></line>
    <line x1="3" y1="21" x2="10" y2="14"></line>
  </svg>`;
  expandButton.addEventListener("click", () => openDiagram(container));
  container.appendChild(expandButton);
}

async function render(): Promise<void> {
  diagrams.forEach(captureDiagramSource);

  let mermaid: typeof import("mermaid").default;
  try {
    ({ default: mermaid } = await import("mermaid"));
  } catch (error) {
    diagrams.forEach(showRenderError);
    console.error("Mermaid load failed:", error);
    return;
  }

  const isLight = document.documentElement.getAttribute("data-mode") !== "dark";
  const fontFamily = getFontFamily();
  const pageBackground = getPageBackground();
  const accent = getThemeColor("--nb-primary", isLight ? "#ff4801" : "#ff6524");

  const lightThemeVariables = {
    fontFamily,
    primaryColor: "#ffffff",
    primaryBorderColor: accent,
    primaryTextColor: "#1d1d1d",
    secondaryColor: "#f3f4f6",
    secondaryBorderColor: "#9ca3af",
    secondaryTextColor: "#1d1d1d",
    tertiaryColor: "#f3f4f6",
    tertiaryBorderColor: "#9ca3af",
    tertiaryTextColor: "#1d1d1d",
    lineColor: accent,
    textColor: "#1d1d1d",
    mainBkg: "#ffffff",
    errorBkgColor: "#fef2f2",
    errorTextColor: "#7f1d1d",
    edgeLabelBackground: pageBackground,
    labelBackground: pageBackground,
  };

  const darkThemeVariables = {
    fontFamily,
    primaryColor: "#1f1f1f",
    primaryBorderColor: accent,
    primaryTextColor: "#f2f2f2",
    secondaryColor: "#262626",
    secondaryBorderColor: "#6b7280",
    secondaryTextColor: "#f2f2f2",
    tertiaryColor: "#262626",
    tertiaryBorderColor: "#6b7280",
    tertiaryTextColor: "#f2f2f2",
    lineColor: accent,
    textColor: "#f2f2f2",
    mainBkg: "#1f1f1f",
    background: "#0f0f0f",
    errorBkgColor: "#3c0501",
    errorTextColor: "#ffefee",
    edgeLabelBackground: pageBackground,
    labelBackground: pageBackground,
  };

  try {
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: "strict",
      theme: "base",
      themeVariables: isLight ? lightThemeVariables : darkThemeVariables,
      flowchart: {
        htmlLabels: true,
        useMaxWidth: true,
        curve: "linear",
      },
    });
  } catch (error) {
    diagrams.forEach(showRenderError);
    console.error("Mermaid initialize failed:", error);
    return;
  }

  for (const diagram of diagrams) {
    try {
      const definition = diagram.getAttribute("data-diagram") as string;
      const { svg } = await mermaid.render(uniqueMermaidId(), definition);
      const sanitized = DOMPurify.sanitize(svg, {
        RETURN_DOM_FRAGMENT: true,
        USE_PROFILES: { html: true, svg: true, svgFilters: true },
      });
      diagram.replaceChildren(sanitized);
      diagram.removeAttribute("data-error");
      wrapDiagram(diagram);
      diagram.setAttribute("data-processed", "true");
    } catch (error) {
      showRenderError(diagram);
      console.error("Mermaid render failed:", error);
    }
  }
}

const observer = new MutationObserver(() => {
  if (diagrams.length > 0) void render();
});

observer.observe(document.documentElement, {
  attributes: true,
  attributeFilter: ["data-mode"],
});

function setup(): void {
  diagrams = Array.from(document.querySelectorAll<HTMLPreElement>("pre.mermaid"));
  dialog = null;
  if (diagrams.length > 0) void render();
}

setup();
