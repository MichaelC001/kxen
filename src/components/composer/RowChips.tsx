// 框外附件 chip 道：图片缩略图 / 文件 / 知识引用，点 X 移除。
import { For, Show } from "solid-js";
import { Image as ImageIcon, X } from "lucide-solid";

export interface RowChip {
  id: string;
  kind: "image" | "knowledge" | "file" | "dir" | "web" | "docs";
  ref: string;
  label: string;
  /** tooltip 展示完整路径/URL（label 只显示 basename，路径长在 chip 上放不下）。 */
  title?: string;
  preview?: string;
}

export default function RowChips(props: { chips: RowChip[]; onRemove: (id: string) => void }) {
  return (
    <Show when={props.chips.length > 0}>
      <div class="flex flex-wrap gap-1.5 px-3 pt-2.5">
        <For each={props.chips}>
          {(chip) => (
            <span
              class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded border border-[var(--border)] bg-[var(--bg-overlay)] text-2xs"
              title={chip.title}
            >
              <Show
                when={chip.preview}
                fallback={chip.kind === "image" ? <ImageIcon size={11} /> : null}
              >
                <img src={chip.preview} alt="" class="w-4 h-4 rounded object-cover" />
              </Show>
              <span class="max-w-32 truncate">{chip.label}</span>
              <button
                class="text-[var(--text-faint)] hover:text-[var(--err)]"
                onClick={() => props.onRemove(chip.id)}
              >
                <X size={11} />
              </button>
            </span>
          )}
        </For>
      </div>
    </Show>
  );
}
