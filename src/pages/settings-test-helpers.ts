export function btnByText(text: string): HTMLButtonElement {
  const found = [...document.body.querySelectorAll<HTMLButtonElement>("button")].find(
    (button) => button.textContent === text,
  );
  if (!found) throw new Error(`button not found: ${text}`);
  return found;
}

export function experimentToggles(text: "已启用" | "已关闭"): HTMLButtonElement[] {
  const heading = [...document.body.querySelectorAll<HTMLDivElement>("div")].find(
    (element) => element.textContent === "实验能力与数据边界",
  );
  if (!heading?.parentElement) throw new Error("experimental settings section not found");
  return [...heading.parentElement.querySelectorAll<HTMLButtonElement>("button")].filter(
    (button) => button.textContent === text,
  );
}
