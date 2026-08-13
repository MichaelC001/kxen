/**
 * Implements automatic activation for a WAI-ARIA tablist. The owning page keeps
 * the tab state; this helper only normalizes keyboard navigation and triggers
 * the same click path used by pointer interaction.
 */
export function onTabKeyDown(event: KeyboardEvent): void {
  const current = event.currentTarget;
  if (!(current instanceof HTMLElement)) return;
  const tablist = current.closest<HTMLElement>("[role='tablist']");
  if (!tablist) return;

  const tabs = [...tablist.querySelectorAll<HTMLElement>("[role='tab']:not([disabled])")];
  const currentIndex = tabs.indexOf(current);
  if (currentIndex < 0 || tabs.length === 0) return;

  const orientation = tablist.getAttribute("aria-orientation") ?? "horizontal";
  let nextIndex: number | undefined;
  if (event.key === "Home") nextIndex = 0;
  else if (event.key === "End") nextIndex = tabs.length - 1;
  else if (
    (orientation === "horizontal" && event.key === "ArrowRight") ||
    (orientation === "vertical" && event.key === "ArrowDown")
  )
    nextIndex = (currentIndex + 1) % tabs.length;
  else if (
    (orientation === "horizontal" && event.key === "ArrowLeft") ||
    (orientation === "vertical" && event.key === "ArrowUp")
  )
    nextIndex = (currentIndex - 1 + tabs.length) % tabs.length;

  if (nextIndex === undefined) return;
  event.preventDefault();
  tabs[nextIndex]?.focus();
  tabs[nextIndex]?.click();
}
