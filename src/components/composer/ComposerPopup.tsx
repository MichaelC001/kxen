// 触发补全弹层：fixed 定位（bottom 锚定向上展开）+ 键盘选中态。
import { For, Show } from "solid-js";
import type { PopupItem } from "./triggers";

export default function ComposerPopup(props: {
  items: PopupItem[];
  selected: number;
  pos: { left: number; bottom: number } | null;
}) {
  return (
    <div
      class="composer-popup fixed w-96 max-h-80 overflow-auto rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] z-30"
      style={
        props.pos
          ? `left:${props.pos.left}px;bottom:${props.pos.bottom}px`
          : "left:16px;bottom:120px"
      }
    >
      <For each={props.items}>
        {(item, i) => (
          <button
            class="w-full flex flex-col items-start gap-0.5 px-3 py-2 text-left hover:bg-[var(--bg-overlay)]"
            classList={{ "bg-[var(--bg-overlay)]": i() === props.selected }}
            onClick={() => item.apply()}
          >
            <span class="w-full text-left truncate">{item.label}</span>
            <Show when={item.detail}>
              <span class="w-full text-2xs text-[var(--text-faint)] leading-snug">
                {item.detail}
              </span>
            </Show>
          </button>
        )}
      </For>
    </div>
  );
}
