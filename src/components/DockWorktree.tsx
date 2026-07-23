// worktree 看板：隔离树 = 并行工作单元（分支 + 脏文件计数 + 切换工作区 + 清理）。
import { createSignal, For, onMount, Show } from "solid-js";
import { Check, GitBranch, Plus, Trash2 } from "lucide-solid";
import {
  workspaceSwitch,
  worktreeCreate,
  worktreeList,
  worktreeRemove,
  worktreeStatus,
  type WorktreeInfo,
} from "../lib/chat";
import { statusline } from "../lib/chat";
import EmptyLine from "./EmptyLine";

interface Row extends WorktreeInfo {
  dirty: number;
}

export default function DockWorktree() {
  const [trees, setTrees] = createSignal<Row[]>([]);
  const [active, setActive] = createSignal("");
  const [name, setName] = createSignal("");
  const [note, setNote] = createSignal("");

  const reload = async () => {
    const [list, sl] = await Promise.all([
      worktreeList().catch(() => []),
      statusline("").catch(() => null),
    ]);
    if (sl) setActive(sl.workdir);
    setTrees(
      await Promise.all(
        list.map(async (t) => ({ ...t, dirty: (await worktreeStatus(t.path)).length })),
      ),
    );
  };
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

  const remove = async (t: Row, withBranch: boolean) => {
    await worktreeRemove(t.name, withBranch).catch(() => {});
    await reload();
    flash(withBranch ? `已删除 ${t.branch}` : "已移除 worktree（分支保留）");
  };

  const switchTo = async (t: Row) => {
    await workspaceSwitch(t.path).catch(() => {});
    setActive(t.path);
    flash(`已切换到 ${t.branch}`);
  };

  return (
    <div class="border-b border-[var(--border)] px-3 py-3">
      <div class="text-2xs uppercase tracking-wider text-[var(--text-faint)] mb-2 flex items-center gap-1.5">
        <GitBranch size={11} class="text-[var(--text-faint)]" />
        worktree 并行看板
      </div>
      <Show when={note()}>
        <div class="text-2xs text-[var(--ok)] mb-1.5">{note()}</div>
      </Show>
      <div class="space-y-1">
        <For each={trees()} fallback={<EmptyLine text="无隔离树" />}>
          {(t) => (
            <div class="group flex items-center gap-1.5 text-xs">
              <Show when={t.path === active()}>
                <Check size={11} class="text-[var(--ok)] shrink-0" />
              </Show>
              <span class="font-mono flex-1 truncate" title={t.path}>
                {t.branch}
              </span>
              <Show when={t.dirty > 0}>
                <span class="text-2xs tabular-nums text-[var(--warn)]">{t.dirty} 改</span>
              </Show>
              <Show when={t.path !== active()}>
                <button
                  class="opacity-0 group-hover:opacity-100 pressable px-1 rounded text-2xs text-[var(--text-faint)] hover:text-[var(--text)]"
                  title="切换工作区到此树（会话跑在该隔离目录）"
                  onClick={() => void switchTo(t)}
                >
                  切换
                </button>
              </Show>
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
