// ModelPill：模型选择 pill + popover（composer action bar 右侧）。
import { createSignal, Show } from "solid-js";
import { ChevronDown } from "lucide-solid";
import { currentModel, setModel } from "../../lib/chat";
import { onMount } from "solid-js";

const PRESETS = [
  { provider: "anthropic", model: "claude-sonnet-4-5-20250929", label: "Claude Sonnet" },
  { provider: "openai", model: "gpt-5.4", label: "GPT (Codex)" },
  { provider: "xai", model: "grok-build-0.1", label: "Grok Build" },
  { provider: "kimi-for-coding", model: "kimi-for-coding", label: "Kimi Code" },
];

export default function ModelPill() {
  const [label, setLabel] = createSignal("");
  const [open, setOpen] = createSignal(false);

  onMount(async () => {
    const m = await currentModel();
    setLabel(`${m.provider}/${m.model}`);
  });

  const current = () =>
    PRESETS.find((p) => `${p.provider}/${p.model}` === label())?.label ?? label();

  return (
    <div class="relative">
      <button
        class="pressable flex items-center gap-1 px-2 py-1 rounded-md text-xs text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60"
        onClick={() => setOpen(!open())}
      >
        <span class="max-w-36 truncate">{current()}</span>
        <ChevronDown size={12} />
      </button>
      <Show when={open()}>
        <div class="composer-popup absolute bottom-full right-0 mb-1.5 w-48 rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] shadow-xl shadow-black/30 overflow-hidden z-20">
          {PRESETS.map((p) => (
            <button
              class="w-full px-3 py-1.5 text-left text-xs hover:bg-[var(--bg-overlay)]"
              classList={{ "text-[var(--accent-hover)]": `${p.provider}/${p.model}` === label() }}
              onClick={() => {
                void setModel(p.provider, p.model);
                setLabel(`${p.provider}/${p.model}`);
                setOpen(false);
              }}
            >
              <div class="font-medium">{p.label}</div>
              <div class="text-2xs text-[var(--text-faint)]">{p.provider}</div>
            </button>
          ))}
        </div>
      </Show>
    </div>
  );
}
