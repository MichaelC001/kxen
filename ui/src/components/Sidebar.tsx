import { A } from "@solidjs/router";
import { createSignal, onMount } from "solid-js";
import { currentModel } from "../lib/chat";

/** 左栏：会话列表（当前单会话占位，多会话持久化接入后展开）+ 底部应用级入口。 */
export default function Sidebar() {
  const [model, setModel] = createSignal("");
  onMount(async () => {
    const m = await currentModel();
    setModel(`${m.provider}/${m.model}`);
  });

  return (
    <nav class="w-52 shrink-0 flex flex-col border-r border-[var(--border)] bg-[var(--bg-raised)]">
      <div class="px-4 pt-4 pb-2 text-lg font-semibold tracking-tight text-[var(--accent-hover)]">
        kxen
      </div>
      <div class="px-3 pb-2">
        <button class="pressable w-full px-3 py-1.5 rounded-md text-sm text-left border border-[var(--border)] text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60">
          + 新会话
        </button>
      </div>
      <div class="flex-1 overflow-auto px-2 space-y-0.5">
        <div class="px-3 py-1.5 rounded-md text-sm bg-[var(--bg-overlay)] text-[var(--text)]">
          当前会话
        </div>
      </div>
      <div class="px-3 py-2 border-t border-[var(--border)] space-y-2">
        <A
          href="/doctor"
          class="block px-1 text-xs text-[var(--text-dim)] hover:text-[var(--text)]"
        >
          环境检查
        </A>
        <div>
          <div class="text-[10px] uppercase tracking-wider text-[var(--text-faint)]">当前模型</div>
          <div class="text-xs text-[var(--text-dim)] truncate" title={model()}>
            {model() || "…"}
          </div>
        </div>
      </div>
    </nav>
  );
}
