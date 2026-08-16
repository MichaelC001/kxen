// flash 全局宿主：fixed 右下角堆叠，App.tsx 挂载一次。
import { For, Show } from "solid-js";
import { X } from "lucide-solid";
import { flash } from "../lib/flash";

export default function FlashHost() {
  return (
    <div class="fixed bottom-4 right-4 z-[90] flex flex-col gap-1.5 items-end">
      <For each={flash.msgs()}>
        {(m) => (
          <div
            class="px-2.5 py-1.5 rounded-md text-xs border shadow-lg flex items-center gap-1.5 max-w-80 text-left"
            classList={{
              "bg-[var(--bg-raised)] border-[var(--border)] text-[var(--text)]": m.kind === "ok",
              "bg-[var(--bg-raised)] border-[var(--err)] text-[var(--err)]": m.kind === "err",
            }}
          >
            <span class="flex-1">{m.text}</span>
            <Show when={m.action}>
              {(action) => (
                <button
                  class="pressable shrink-0 px-1.5 py-0.5 rounded border border-[var(--border)] text-2xs text-[var(--accent)]"
                  onClick={() => action().run()}
                >
                  {action().label}
                </button>
              )}
            </Show>
            <button
              class="pressable shrink-0 opacity-60 hover:opacity-100"
              onClick={() => flash.dismiss(m.id)}
              title="关闭"
            >
              <X size={11} />
            </button>
          </div>
        )}
      </For>
    </div>
  );
}
