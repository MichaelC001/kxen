// 弹层键盘导航：上下移选中 / Enter、Tab 应用 / Esc 关闭。
// 返回是否已消费：未消费的键继续走发送守卫与语音 PTT（弹层开着时打字不应断输入）。
import type { PopupState, Trigger } from "./triggers";

export function handlePopupKey(
  e: KeyboardEvent,
  p: PopupState & Trigger,
  setPopup: (p: (PopupState & Trigger) | null) => void,
): boolean {
  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    const delta = e.key === "ArrowDown" ? 1 : -1;
    setPopup({ ...p, selected: (p.selected + delta + p.items.length) % p.items.length });
    return true;
  }
  if (e.key === "Enter" || e.key === "Tab") {
    e.preventDefault();
    p.items[p.selected]?.apply();
    return true;
  }
  if (e.key === "Escape") {
    e.preventDefault();
    setPopup(null);
    return true;
  }
  return false;
}
