// ModelPicker：catalog 驱动（models.dev 快照）——显示名 + id + ctx + 能力徽章 + 搜索 + 角色分配。
import { createSignal, For, onMount, Show } from "solid-js";
import { Check, ChevronDown, Search } from "lucide-solid";
import { configSetRole, currentModel, setModel } from "../../lib/chat";
import { onClickOutside } from "../../lib/dismiss";
import {
  fmtCtx,
  modelOf,
  modelsCatalog,
  type ModelInfo,
  type ProviderCatalog,
} from "../../lib/models";

const ROLE_ASSIGN: Array<{ role: string; label: string }> = [
  { role: "chat", label: "设为主会话模型" },
  { role: "thinking", label: "设为思考模型" },
  { role: "planning", label: "设为规划模型" },
  { role: "execution", label: "设为执行模型" },
  { role: "review", label: "设为审查模型" },
  { role: "research", label: "设为调研模型" },
];

interface Row {
  provider: string;
  providerName: string;
  model: ModelInfo;
}

export default function ModelPicker() {
  const [cur, setCur] = createSignal({ provider: "", model: "" });
  const [cat, setCat] = createSignal<ProviderCatalog[]>([]);
  const [open, setOpen] = createSignal(false);
  const [query, setQuery] = createSignal("");
  const [roleMsg, setRoleMsg] = createSignal("");
  let root: HTMLDivElement | undefined;
  onClickOutside(
    () => root,
    () => setOpen(false),
  );

  onMount(async () => {
    const [m, c] = await Promise.all([currentModel(), modelsCatalog()]);
    setCur({ provider: m.provider, model: m.model });
    setCat(c);
  });

  const rows = (): Row[] =>
    cat().flatMap((p) =>
      p.models.map((model) => ({ provider: p.provider, providerName: p.provider_name, model })),
    );
  const filtered = () => {
    const q = query().toLowerCase();
    if (!q) return rows();
    return rows().filter((r) =>
      `${r.providerName} ${r.model.name} ${r.model.id}`.toLowerCase().includes(q),
    );
  };

  const curInfo = () => modelOf(cat(), cur().provider, cur().model);
  const curLabel = () =>
    curInfo()?.name ?? (cur().model ? `${cur().provider}/${cur().model}` : "模型");

  const pick = (r: Row) => {
    void setModel(r.provider, r.model.id);
    setCur({ provider: r.provider, model: r.model.id });
    setOpen(false);
  };

  const assignRole = (role: string, label: string) => {
    if (!cur().model) return;
    void configSetRole(role, cur().provider, cur().model).then(() => {
      setRoleMsg(`${curLabel()} → ${label.replace("设为", "")} ✓`);
      setTimeout(() => setRoleMsg(""), 1800);
    });
  };

  return (
    <div class="relative" ref={(el) => (root = el)}>
      <button class="pressable model-pill" onClick={() => setOpen(!open())}>
        <span class="text-2xs text-[var(--text-faint)]">{curInfo()?.family ?? cur().provider}</span>
        <span class="model-pill-name">{curLabel()}</span>
        <Show when={curInfo()?.context}>
          <span class="text-2xs text-[var(--text-faint)]">{fmtCtx(curInfo()!.context)}</span>
        </Show>
        <ChevronDown size={12} />
      </button>
      <Show when={roleMsg()}>
        <span class="text-2xs text-[var(--ok)]">{roleMsg()}</span>
      </Show>

      <Show when={open()}>
        <div class="composer-popup absolute bottom-full right-0 mb-1.5 w-80 rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] overflow-hidden z-20">
          <div class="flex items-center gap-1.5 px-2.5 py-1.5 border-b border-[var(--border)]">
            <Search size={12} class="text-[var(--text-faint)]" />
            <input
              class="flex-1 bg-transparent text-xs focus:outline-none placeholder:text-[var(--text-faint)]"
              placeholder="搜索模型（名称 / id）…"
              value={query()}
              onInput={(e) => setQuery(e.currentTarget.value)}
            />
          </div>
          <div class="max-h-72 overflow-y-auto py-1">
            <For each={filtered()}>
              {(r) => (
                <div
                  class="model-row"
                  classList={{
                    "model-row-active": r.model.id === cur().model && r.provider === cur().provider,
                  }}
                  onClick={() => pick(r)}
                  onContextMenu={(e) => e.preventDefault()}
                >
                  <div class="flex-1 min-w-0">
                    <div class="flex items-center gap-1.5">
                      <span class="text-xs font-medium truncate">{r.model.name}</span>
                      <Show when={r.model.reasoning}>
                        <span class="text-2xs px-1 rounded border border-[var(--border)] text-[var(--text-faint)]">
                          推理
                        </span>
                      </Show>
                      <Show when={r.model.modalities_in.some((m) => m !== "text")}>
                        <span class="text-2xs px-1 rounded border border-[var(--border)] text-[var(--text-faint)]">
                          {r.model.modalities_in.filter((m) => m !== "text").join("/")}
                        </span>
                      </Show>
                      <Show when={r.model.id === cur().model && r.provider === cur().provider}>
                        <Check size={12} class="text-[var(--accent-hover)]" />
                      </Show>
                    </div>
                    <div class="text-2xs text-[var(--text-faint)] truncate">
                      {r.providerName} · {r.model.id} · ctx {fmtCtx(r.model.context)}
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
                {(r) => (
                  <button
                    class="role-chip"
                    disabled={!cur().model}
                    onClick={() => assignRole(r.role, r.label)}
                  >
                    {r.label.replace("设为", "")}
                  </button>
                )}
              </For>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
}
