// 知识注入控制台：模型实际看到什么（注入预览）+ 启停（不删只关）+ scope 晋升 + 去重提示。
import { createSignal, For, onMount, Show } from "solid-js";
import { ChevronDown, ChevronRight, Eye, Trash2 } from "lucide-solid";
import EmptyLine from "../EmptyLine";
import {
  knowledgeAdd,
  knowledgeInjectionPreview,
  knowledgeList,
  knowledgeMove,
  knowledgeRemove,
  knowledgeSetEnabled,
  type KnowledgeEntry,
} from "../../lib/knowledge";
import { badgeChip } from "../../lib/variants";

const SCOPES = [
  { id: "project", label: "项目" },
  { id: "global", label: "全局" },
  { id: "memory", label: "本机" },
];
const KINDS = ["correction", "convention", "pitfall", "preference", "note"];

export default function KnowledgeSection() {
  const [entries, setEntries] = createSignal<KnowledgeEntry[]>([]);
  const [preview, setPreview] = createSignal<{
    project: string | null;
    extra: string | null;
  } | null>(null);
  const [showPreview, setShowPreview] = createSignal(false);
  const [scope, setScope] = createSignal("memory");
  const [kind, setKind] = createSignal("convention");
  const [desc, setDesc] = createSignal("");
  const [content, setContent] = createSignal("");
  const [saved, setSaved] = createSignal("");

  const reload = async () => {
    const [list, prev] = await Promise.all([
      knowledgeList().catch(() => []),
      knowledgeInjectionPreview().catch(() => null),
    ]);
    setEntries(list);
    setPreview(prev);
  };
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

  const toggle = async (e: KnowledgeEntry) => {
    await knowledgeSetEnabled(e.scope, e.slug, !e.enabled);
    await reload();
  };

  const move = async (e: KnowledgeEntry, to: string) => {
    if (to === e.scope) return;
    await knowledgeMove(e.scope, e.slug, to);
    await reload();
    flash(`已移动到 ${to}`);
  };

  const remove = async (e: KnowledgeEntry) => {
    await knowledgeRemove(e.scope, e.slug);
    await reload();
    flash("已删除（回收站）");
  };

  const countOf = (s: string) => entries().filter((e) => e.scope === s).length;
  const enabledOf = (s: string) => entries().filter((e) => e.scope === s && e.enabled).length;
  const dupes = () => {
    const bySlug = new Map<string, Set<string>>();
    for (const e of entries()) {
      bySlug.set(e.slug, new Set([...(bySlug.get(e.slug) ?? []), e.scope]));
    }
    return [...bySlug.entries()].filter(([, scopes]) => scopes.size > 1).map(([slug]) => slug);
  };

  return (
    <>
      <div class="flex items-center justify-between">
        <div class="text-xs text-[var(--text-faint)]">
          项目 {enabledOf("project")}/{countOf("project")} · 全局 {enabledOf("global")}/
          {countOf("global")} · 本机 {enabledOf("memory")}/{countOf("memory")}（启用/总条数）
        </div>
        <button
          class="pressable flex items-center gap-1 px-2 py-1 rounded text-xs text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60"
          onClick={() => setShowPreview(!showPreview())}
        >
          <Eye size={12} />
          {showPreview() ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
          注入预览
        </button>
      </div>
      <Show when={saved()}>
        <div class="text-xs text-[var(--ok)]">{saved()}</div>
      </Show>

      <Show when={showPreview()}>
        <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-3 max-h-72 overflow-auto">
          <div class="text-2xs text-[var(--text-faint)] mb-1.5">
            模型下轮 system prompt 实际看到的知识文本（启停即时生效）
          </div>
          <pre class="text-2xs font-mono whitespace-pre-wrap text-[var(--text-dim)]">
            {preview()?.project ?? "（无项目知识）"}
            {"\n"}
            {preview()?.extra ?? "（无全局/本机知识）"}
          </pre>
        </div>
      </Show>

      <Show when={dupes().length > 0}>
        <div class="rounded-lg border border-[var(--warn)]/40 bg-[var(--warn)]/10 px-3 py-2 text-xs text-[var(--warn)]">
          同 slug 跨 scope 共存（注入时会重复）：{dupes().join("、")} — 建议移动合并或停用其一
        </div>
      </Show>

      <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] divide-y divide-[var(--border)]">
        <For each={entries()} fallback={<EmptyLine text="暂无知识条目" />}>
          {(e) => (
            <div class="flex items-start gap-2 px-4 py-3" classList={{ "opacity-45": !e.enabled }}>
              <button
                class="pressable mt-0.5 w-7 h-4 rounded-full relative shrink-0 transition-colors"
                classList={{
                  "bg-[var(--accent)]": e.enabled,
                  "bg-[var(--bg-overlay)]": !e.enabled,
                }}
                title={e.enabled ? "停用（注入即刻跳过）" : "启用"}
                onClick={() => void toggle(e)}
              >
                <span
                  class="absolute top-0.5 w-3 h-3 rounded-full bg-white transition-all"
                  style={e.enabled ? "left:14px" : "left:2px"}
                />
              </button>
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
              <select
                class="bg-transparent border border-[var(--border)] rounded px-1 py-0.5 text-2xs text-[var(--text-dim)]"
                value={e.scope}
                title="移动到其他 scope"
                onChange={(e2) => void move(e, e2.currentTarget.value)}
              >
                {SCOPES.map((s) => (
                  <option value={s.id}>{s.label}</option>
                ))}
              </select>
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

      <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-4 space-y-2">
        <div class="text-xs text-[var(--text-faint)]">手动添加（Agent 自主沉淀共用同一存储）</div>
        <div class="flex gap-2">
          <select
            class="bg-transparent border border-[var(--border)] rounded px-1.5 py-1 text-xs text-[var(--text-dim)]"
            value={scope()}
            onChange={(e) => setScope(e.currentTarget.value)}
          >
            <option value="project">项目（克制）</option>
            <option value="global">全局</option>
            <option value="memory">本机</option>
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
    </>
  );
}
