// 弹层点外关闭（capture 阶段 mousedown：先于按钮 click 触发，不误伤自身的开合按钮）。
import { onCleanup } from "solid-js";

export function onClickOutside(
  inside: () => HTMLElement | undefined | null,
  onOutside: () => void,
) {
  const handler = (e: MouseEvent) => {
    const el = inside();
    if (el && e.target instanceof Node && !el.contains(e.target)) onOutside();
  };
  window.addEventListener("mousedown", handler, true);
  onCleanup(() => window.removeEventListener("mousedown", handler, true));
}
