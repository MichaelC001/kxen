import { A } from "@solidjs/router";
import { For, createSignal, onMount } from "solid-js";
import { Activity, Moon, Plus, Settings as SettingsIcon, Sun, X } from "lucide-solid";
import { currentModel, sessionDelete } from "../lib/chat";
import {
  activeSessionId,
  initSessions,
  newSession,
  refreshSessions,
  sessions,
  switchSession,
} from "../lib/state";
import { theme, toggleTheme } from "../lib/theme";

/** 左栏：会话列表（会话是家）+ 底部应用级入口。 */
export default function Sidebar() {
  const [model, setModel] = createSignal("");
  onMount(async () => {
    await initSessions();
    const m = await currentModel();
    setModel(`${m.provider}/${m.model}`);
  });

  const remove = async (id: string) => {
    await sessionDelete(id);
    await refreshSessions();
  };

  return (
    <nav class="w-52 shrink-0 flex flex-col border-r border-[var(--border)] bg-[var(--bg-raised)]">
      <div class="traffic-pad" data-tauri-drag-region />
      <div class="px-4 pb-2 text-lg font-semibold tracking-tight text-[var(--accent-hover)]">
        kxen
      </div>
      <div class="px-3 pb-2">
        <button
          class="pressable w-full px-3 py-1.5 rounded-md text-sm text-left border border-[var(--border)] text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60 flex items-center gap-2"
          onClick={() => void newSession()}
        >
          <Plus size={14} />
          新会话
        </button>
      </div>
      <div class="flex-1 overflow-auto px-2 space-y-0.5">
        <For each={sessions()}>
          {(s) => (
            <div
              class="group flex items-center rounded-md text-sm cursor-pointer"
              classList={{
                "bg-[var(--bg-overlay)] text-[var(--text)]": s.id === activeSessionId(),
                "text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60":
                  s.id !== activeSessionId(),
              }}
              onClick={() => switchSession(s.id)}
            >
              <span class="flex-1 px-3 py-1.5 truncate" title={s.title}>
                {s.title}
              </span>
              <button
                class="px-2 text-[var(--text-faint)] opacity-0 group-hover:opacity-100 hover:text-[var(--err)]"
                title="删除会话"
                onClick={(e) => {
                  e.stopPropagation();
                  void remove(s.id);
                }}
              >
                <X size={13} />
              </button>
            </div>
          )}
        </For>
      </div>
      <div class="px-3 py-2 border-t border-[var(--border)] space-y-2">
        <div class="flex items-center justify-between">
          <A
            href="/settings"
            class="px-1 text-xs text-[var(--text-dim)] hover:text-[var(--text)] flex items-center gap-1.5"
          >
            <SettingsIcon size={13} />
            设置
          </A>
          <A
            href="/doctor"
            class="px-1 text-xs text-[var(--text-dim)] hover:text-[var(--text)] flex items-center gap-1.5"
          >
            <Activity size={13} />
          </A>
          <button
            class="pressable px-1.5 py-0.5 rounded text-xs text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60 flex items-center"
            title="切换明暗主题"
            onClick={(e) => toggleTheme(e.clientX, e.clientY)}
          >
            {theme() === "dark" ? <Moon size={13} /> : <Sun size={13} />}
          </button>
        </div>
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
