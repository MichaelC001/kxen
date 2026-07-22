// 补全弹窗：分区（命令 / skills / 文件 / 知识）+ 键盘导航 + origin-aware 入场。
import { createSignal, For, Show } from "solid-js";

export interface PopupItem {
  label: string;
  detail?: string | undefined;
  badge?: string | undefined;
  section?: string | undefined;
  apply: () => void;
}

export interface PopupState {
  items: PopupItem[];
  selected: number;
  left: number;
  bottom: number;
}

export function usePopupSelection(): {
  popup: () => PopupState | null;
  open: (items: PopupItem[], rect: DOMRect | null, container: DOMRect) => void;
  close: () => void;
  move: (delta: number) => void;
  applySelected: () => void;
} {
  const [popup, setPopup] = createSignal<PopupState | null>(null);
  return {
    popup,
    open: (items, rect, container) => {
      if (items.length === 0) {
        setPopup(null);
        return;
      }
      // 锚定 caret（无坐标退化到容器左下）
      const left = rect ? Math.max(0, rect.left - container.left) : 0;
      const bottom = rect ? container.bottom - rect.top + 4 : 0;
      setPopup({ items, selected: 0, left, bottom });
    },
    close: () => setPopup(null),
    move: (delta) => {
      const p = popup();
      if (!p) return;
      setPopup({ ...p, selected: (p.selected + delta + p.items.length) % p.items.length });
    },
    applySelected: () => {
      const p = popup();
      p?.items[p.selected]?.apply();
    },
  };
}

export default function ComposerPopup(props: { popup: PopupState }) {
  return (
    <div
      class="composer-popup absolute max-h-64 overflow-y-auto rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] shadow-xl shadow-black/30 z-20 min-w-64"
      style={{ left: `${props.popup.left}px`, bottom: `${props.popup.bottom}px` }}
    >
      <For each={props.popup.items}>
        {(item, i) => (
          <>
            <Show
              when={
                item.section && (i() === 0 || props.popup.items[i() - 1]?.section !== item.section)
              }
            >
              <div class="popup-section">{item.section}</div>
            </Show>
            <button
              class="w-full flex items-center gap-2 px-3 py-1.5 text-left text-xs"
              classList={{
                "bg-[var(--bg-overlay)]": i() === props.popup.selected,
                "text-[var(--text-dim)]": i() !== props.popup.selected,
              }}
              onMouseDown={(e) => {
                e.preventDefault(); // 不抢焦点
                item.apply();
              }}
            >
              <span class="truncate flex-1 font-mono">{item.label}</span>
              <Show when={item.detail}>
                <span class="text-2xs text-[var(--text-faint)] truncate popup-detail">
                  {item.detail}
                </span>
              </Show>
              <Show when={item.badge}>
                <span class="text-2xs px-1 rounded border border-[var(--border)] text-[var(--text-faint)]">
                  {item.badge}
                </span>
              </Show>
            </button>
          </>
        )}
      </For>
    </div>
  );
}
