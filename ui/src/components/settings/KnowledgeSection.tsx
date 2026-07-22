// 知识库人工通道：列表审计 + 手动添加 + 删除（回收站）。
import { createSignal, For, onMount, Show } from "solid-js";
import { Trash2 } from "lucide-solid";
import {
  knowledgeAdd,
  knowledgeList,
  knowledgeRemove,
  type KnowledgeEntry,
} from "../../lib/knowledge";
import { badgeChip } from "../../lib/variants";

const SCOPES = [
  { id: "project", label: "项目（.agents/rules，克制）" },
  { id: "global", label: "全局（~/.agents/rules）" },
  { id: "memory", label: "本机（.kxen/memory）" },
];
const KINDS = ["correction", "convention", "pitfall", "preference", "note"];

export default function KnowledgeSection() {
  const [entries, setEntries] = createSignal<KnowledgeEntry[]>([]);
  const [scope, setScope] = createSignal("memory");
  const [kind, setKind] = createSignal("convention");
  const [desc, setDesc] = createSignal("");
  const [content, setContent] = createSignal("");
  const [saved, setSaved] = createSignal("");

  const reload = async () => setEntries(await knowledgeList().catch(() => []));
  onMount(() => void reload());

  const flash = (msg: string) => {
    setSaved(msg);
    setTimeout(() => setSaved(""), 2000);
  };

  const add = async () => {
    if (!desc().trim() || !content().trim()) return;
    await knowledgeAdd(scope(), kind(), desc().trim(), content().trim());
    setDesc("");
    setContent("");
    await reload();
    flash("已写入知识库");
  };

  const remove = async (e: KnowledgeEntry) => {
    await knowledgeRemove(e.scope, e.slug);
    await reload();
    flash("已删除（回收站）");
  };

  return (
    <>
      <div class="text-xs text-[var(--text-faint)]">
        Agent 自主沉淀（knowledge 工具）与人工添加共用同一存储；同 slug 覆盖更新
      </div>
      <Show when={saved()}>
        <div class="text-xs text-[var(--ok)]">{saved()}</div>
      </Show>
      <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-4 space-y-2">
        <div class="flex gap-2">
          <select
            class="bg-transparent border border-[var(--border)] rounded px-1.5 py-1 text-xs text-[var(--text-dim)]"
            value={scope()}
            onChange={(e) => setScope(e.currentTarget.value)}
          >
            {SCOPES.map((s) => (
              <option value={s.id}>{s.label}</option>
            ))}
          </select>
          <select
            class="bg-transparent border border-[var(--border)] rounded px-1.5 py-1 text-xs text-[var(--text-dim)]"
            value={kind()}
            onChange={(e) => setKind(e.currentTarget.value)}
          >
            {KINDS.map((k) => (
              <option value={k}>{k}</option>
            ))}
          </select>
        </div>
        <input
          class="w-full bg-transparent border border-[var(--border)] rounded px-2 py-1 text-xs"
          placeholder="一句话描述（同题自动生成同 slug 覆盖）"
          value={desc()}
          onInput={(e) => setDesc(e.currentTarget.value)}
        />
        <textarea
          class="w-full bg-transparent border border-[var(--border)] rounded px-2 py-1 text-xs h-16"
          placeholder="正文（原子一条，别写流水账）"
          value={content()}
          onInput={(e) => setContent(e.currentTarget.value)}
        />
        <button
          class="pressable px-3 py-1 rounded-md text-xs border border-[var(--border)] disabled:opacity-40"
          disabled={!desc().trim() || !content().trim()}
          onClick={() => void add()}
        >
          写入知识库
        </button>
      </div>
      <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] divide-y divide-[var(--border)]">
        <For
          each={entries()}
          fallback={<div class="px-4 py-3 text-xs text-[var(--text-faint)]">暂无知识条目</div>}
        >
          {(e) => (
            <div class="flex items-start gap-2 px-4 py-3">
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-1.5 mb-0.5">
                  <span class={badgeChip({ tone: "accent" })}>{e.scope}</span>
                  <span class={badgeChip({ tone: "faint" })}>{e.type}</span>
                  <span class="text-2xs text-[var(--text-faint)]">{e.date}</span>
                </div>
                <div class="text-sm">{e.description}</div>
                <div class="text-xs text-[var(--text-faint)] truncate" title={e.content}>
                  {e.content}
                </div>
              </div>
              <button
                class="pressable px-1.5 py-1 rounded text-[var(--text-faint)] hover:text-[var(--err)]"
                title="删除（回收站）"
                onClick={() => void remove(e)}
              >
                <Trash2 size={13} />
              </button>
            </div>
          )}
        </For>
      </div>
    </>
  );
}
