// worktree 区：隔离树列表 + 新建 + 清理（agent 派发可挂隔离）。
import { createSignal, For, onMount, Show } from "solid-js";
import { GitBranch, Plus, Trash2 } from "lucide-solid";
import { worktreeCreate, worktreeList, worktreeRemove, type WorktreeInfo } from "../lib/chat";
import EmptyLine from "./EmptyLine";

export default function DockWorktree() {
  const [trees, setTrees] = createSignal<WorktreeInfo[]>([]);
  const [name, setName] = createSignal("");
  const [note, setNote] = createSignal("");

  const reload = async () => setTrees(await worktreeList().catch(() => []));
  onMount(() => void reload());

  const flash = (msg: string) => {
    setNote(msg);
    setTimeout(() => setNote(""), 2500);
  };

  const create = async () => {
    const n = name().trim();
    if (!n) return;
    const r = await worktreeCreate(n).catch((e) => String(e));
    if (typeof r === "string") {
      flash(r);
    } else {
      setName("");
      await reload();
      flash(`已创建 ${r.branch}`);
    }
  };

  const remove = async (t: WorktreeInfo, withBranch: boolean) => {
    await worktreeRemove(t.name, withBranch).catch(() => {});
    await reload();
    flash(withBranch ? `已删除 ${t.branch}` : "已移除 worktree（分支保留）");
  };

  return (
    <div class="border-b border-[var(--border)] px-3 py-3">
      <div class="text-2xs uppercase tracking-wider text-[var(--text-faint)] mb-2 flex items-center gap-1.5">
        <GitBranch size={11} class="text-[var(--text-faint)]" />
        worktree 隔离
      </div>
      <Show when={note()}>
        <div class="text-2xs text-[var(--ok)] mb-1.5">{note()}</div>
      </Show>
      <div class="space-y-1">
        <For each={trees()} fallback={<EmptyLine text="无隔离树" />}>
          {(t) => (
            <div class="group flex items-center gap-1.5 text-xs">
              <span class="font-mono flex-1 truncate" title={t.path}>
                {t.branch}
              </span>
              <button
                class="opacity-0 group-hover:opacity-100 pressable px-1 rounded text-[var(--text-faint)] hover:text-[var(--text)]"
                title="移除 worktree（分支保留）"
                onClick={() => void remove(t, false)}
              >
                <Trash2 size={11} />
              </button>
              <button
                class="opacity-0 group-hover:opacity-100 pressable px-1 rounded text-2xs text-[var(--err)]"
                title="移除并删除分支"
                onClick={() => void remove(t, true)}
              >
                删分支
              </button>
            </div>
          )}
        </For>
      </div>
      <div class="flex gap-1.5 mt-2">
        <input
          class="flex-1 bg-transparent border border-[var(--border)] rounded px-1.5 py-1 text-2xs font-mono placeholder:text-[var(--text-faint)]"
          placeholder="新隔离树名（a-z0-9-）"
          value={name()}
          onInput={(e) => setName(e.currentTarget.value)}
          onKeyDown={(e) => e.key === "Enter" && void create()}
        />
        <button
          class="pressable px-1.5 py-1 rounded border border-[var(--border)]"
          onClick={() => void create()}
        >
          <Plus size={12} />
        </button>
      </div>
    </div>
  );
}
