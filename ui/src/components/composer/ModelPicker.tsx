// ModelPicker：富模型选择器——[Provider][名称][信息] + 搜索 + 右键分配角色。
import { createSignal, For, onMount, Show } from "solid-js";
import { Check, ChevronDown, Search } from "lucide-solid";
import { configSetRole, currentModel, setModel } from "../../lib/chat";

interface ModelPreset {
  provider: string;
  brand: string;
  model: string;
  label: string;
  context: string;
  note: string;
}

const PRESETS: ModelPreset[] = [
  {
    provider: "anthropic",
    brand: "Claude",
    model: "claude-sonnet-4-5-20250929",
    label: "Sonnet 4.5",
    context: "200k",
    note: "订阅 · 均衡主力",
  },
  {
    provider: "anthropic",
    brand: "Claude",
    model: "claude-opus-4-5-20251101",
    label: "Opus 4.5",
    context: "200k",
    note: "订阅 · 深度思考",
  },
  {
    provider: "openai",
    brand: "GPT",
    model: "gpt-5.4",
    label: "GPT-5.4",
    context: "400k",
    note: "Codex 订阅",
  },
  {
    provider: "xai",
    brand: "Grok",
    model: "grok-build-0.1",
    label: "Grok Build",
    context: "—",
    note: "订阅 · 高速执行",
  },
  {
    provider: "kimi-for-coding",
    brand: "Kimi",
    model: "kimi-for-coding",
    label: "Kimi Code",
    context: "256k",
    note: "订阅 · 工具调用强",
  },
];

const ROLE_ASSIGN: Array<{ role: string; label: string }> = [
  { role: "chat", label: "设为主会话模型" },
  { role: "thinking", label: "设为思考模型" },
  { role: "planning", label: "设为规划模型" },
  { role: "execution", label: "设为执行模型" },
  { role: "review", label: "设为审查模型" },
  { role: "research", label: "设为调研模型" },
];

export default function ModelPicker() {
  const [label, setLabel] = createSignal("");
  const [open, setOpen] = createSignal(false);
  const [query, setQuery] = createSignal("");
  const [roleMsg, setRoleMsg] = createSignal("");

  onMount(async () => {
    const m = await currentModel();
    setLabel(`${m.provider}/${m.model}`);
  });

  const current = () => PRESETS.find((p) => `${p.provider}/${p.model}` === label());
  const filtered = () => {
    const q = query().toLowerCase();
    if (!q) return PRESETS;
    return PRESETS.filter((p) => `${p.brand} ${p.label} ${p.model}`.toLowerCase().includes(q));
  };

  const pick = (p: ModelPreset) => {
    void setModel(p.provider, p.model);
    setLabel(`${p.provider}/${p.model}`);
    setOpen(false);
  };

  const assignRole = (p: ModelPreset, role: string, roleLabel: string) => {
    void configSetRole(role, p.provider, p.model).then(() => {
      setRoleMsg(`${p.label} → ${roleLabel.replace("设为", "")} ✓`);
      setTimeout(() => setRoleMsg(""), 1800);
    });
  };

  return (
    <div class="relative">
      <button class="pressable model-pill" onClick={() => setOpen(!open())}>
        <span class="text-2xs text-[var(--text-faint)]">{current()?.brand ?? "模型"}</span>
        <span class="model-pill-name">{current()?.label ?? label()}</span>
        <ChevronDown size={12} />
      </button>
      <Show when={roleMsg()}>
        <span class="text-2xs text-[var(--ok)]">{roleMsg()}</span>
      </Show>

      <Show when={open()}>
        <div class="composer-popup absolute bottom-full right-0 mb-1.5 w-72 rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] shadow-xl shadow-black/30 overflow-hidden z-20">
          <div class="flex items-center gap-1.5 px-2.5 py-1.5 border-b border-[var(--border)]">
            <Search size={12} class="text-[var(--text-faint)]" />
            <input
              class="flex-1 bg-transparent text-xs focus:outline-none placeholder:text-[var(--text-faint)]"
              placeholder="搜索模型…"
              value={query()}
              onInput={(e) => setQuery(e.currentTarget.value)}
            />
          </div>
          <div class="max-h-72 overflow-y-auto py-1">
            <For each={filtered()}>
              {(p) => (
                <div
                  class="model-row"
                  classList={{ "model-row-active": `${p.provider}/${p.model}` === label() }}
                  onClick={() => pick(p)}
                  onContextMenu={(e) => e.preventDefault()}
                >
                  <span class="model-brand">{p.brand}</span>
                  <div class="flex-1 min-w-0">
                    <div class="flex items-center gap-1.5">
                      <span class="text-xs font-medium truncate">{p.label}</span>
                      <Show when={`${p.provider}/${p.model}` === label()}>
                        <Check size={12} class="text-[var(--accent-hover)]" />
                      </Show>
                    </div>
                    <div class="text-2xs text-[var(--text-faint)] truncate">
                      {p.model} · ctx {p.context} · {p.note}
                    </div>
                  </div>
                </div>
              )}
            </For>
            <Show when={filtered().length === 0}>
              <div class="px-3 py-2 text-2xs text-[var(--text-faint)]">无匹配模型</div>
            </Show>
          </div>
          <div class="border-t border-[var(--border)] px-2.5 py-1.5">
            <div class="text-2xs text-[var(--text-faint)] mb-1">把当前模型分配为…</div>
            <div class="flex flex-wrap gap-1">
              <For each={ROLE_ASSIGN}>
                {(r) => {
                  const p = current();
                  return (
                    <button
                      class="role-chip"
                      disabled={!p}
                      onClick={() => p && assignRole(p, r.role, r.label)}
                    >
                      {r.label.replace("设为", "")}
                    </button>
                  );
                }}
              </For>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
}
