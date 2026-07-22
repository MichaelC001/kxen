// MicMenu：语音引擎快捷切换（状态点 + 未配置明示，切换即热生效）。
import { createSignal, For, onMount, Show } from "solid-js";
import { ChevronDown } from "lucide-solid";
import { setVoiceEngine, voiceEngines, type VoiceOverview } from "../../lib/voice";
import { statusDot } from "../../lib/variants";

const TONE: Record<string, "ok" | "warn" | "err" | "faint"> = {
  ready: "ok",
  needs_auth: "warn",
  unconfigured: "warn",
  unavailable: "err",
};

export default function MicMenu(props: { onEngine: (id: string) => void }) {
  const [open, setOpen] = createSignal(false);
  const [overview, setOverview] = createSignal<VoiceOverview | null>(null);

  const reload = async () => setOverview(await voiceEngines().catch(() => null));
  onMount(() => void reload());

  const pick = async (id: string) => {
    await setVoiceEngine(id, overview()?.fallback ?? []).catch(() => {});
    await reload();
    props.onEngine(id);
    setOpen(false);
  };

  return (
    <div class="relative">
      <button class="pressable action-icon" title="语音引擎" onClick={() => setOpen(!open())}>
        <ChevronDown size={12} />
      </button>
      <Show when={open()}>
        <div class="composer-popup absolute bottom-full right-0 mb-1.5 w-52 rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] shadow-xl shadow-black/30 overflow-hidden z-20">
          <div class="popup-section">语音引擎</div>
          <For each={overview()?.engines ?? []}>
            {(e) => (
              <button
                class="popup-row"
                classList={{
                  "opacity-50": e.status === "unconfigured" || e.status === "unavailable",
                }}
                onClick={() => void pick(e.id)}
              >
                <span class={statusDot({ tone: TONE[e.status] ?? "faint" })} />
                <span class="flex-1 text-left truncate" title={e.detail}>
                  {e.label}
                </span>
                <Show when={overview()?.engine === e.id}>
                  <span class="text-2xs text-[var(--accent-hover)]">当前</span>
                </Show>
                <Show when={e.status === "unconfigured"}>
                  <span class="text-2xs text-[var(--text-faint)]">未配置</span>
                </Show>
              </button>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}
