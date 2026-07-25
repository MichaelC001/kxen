// worktree 看板：隔离树 = 并行工作单元（分支 + 脏文件计数 + 切换工作区 + 清理）。
import { createSignal, For, onMount, Show } from "solid-js";
import { Check, GitBranch, Plus, Trash2 } from "lucide-solid";
import {
  statusline,
  workspaceSwitch,
  worktreeCreate,
  worktreeList,
  worktreeRemove,
  worktreeStatus,
  type WorktreeInfo,
} from "../lib/chat";
import { createAction } from "../lib/async-guard";
import { flashErr, flashOk } from "../lib/flash";
import EmptyLine from "./EmptyLine";

interface Row extends WorktreeInfo {
  dirty: number;
}

/** 删除确认条状态：dirty（有改动可丢）或删分支（不可恢复）时先经行内确认（RewindConfirm 模式）。 */
interface PendingRemove {
  name: string;
  branch: string;
  withBranch: boolean;
  dirty: number;
}

function confirmText(r: PendingRemove): string {
  const parts: string[] = [];
  if (r.dirty > 0) parts.push(`${r.dirty} 处未提交改动将丢失`);
  if (r.withBranch) parts.push(`分支 ${r.branch} 将被删除（不可恢复）`);
  return `确认移除 ${r.name}：${parts.join("，")}。`;
}

export default function DockWorktree() {
  const [trees, setTrees] = createSignal<Row[]>([]);
  const [active, setActive] = createSignal("");
  const [name, setName] = createSignal("");
  const [pendingRemove, setPendingRemove] = createSignal<PendingRemove | null>(null);
  const removeAction = createAction();
  const switchAction = createAction();

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

  const create = async () => {
    const n = name().trim();
    if (!n) return;
    try {
      const r = await worktreeCreate(n);
      setName("");
      await reload();
      flashOk(`已创建 ${r.branch}`);
    } catch (e) {
      flashErr(`创建失败：${e instanceof Error ? e.message : String(e)}`);
    }
  };

  const doRemove = (r: PendingRemove) =>
    removeAction.run(() => worktreeRemove(r.name, r.withBranch), {
      okText: r.withBranch ? `已删除 ${r.branch}` : "已移除 worktree（分支保留）",
      errPrefix: "删除失败",
      onOk: () => void reload(),
    });

  const requestRemove = (t: Row, withBranch: boolean) => {
    const r = { name: t.name, branch: t.branch, withBranch, dirty: t.dirty };
    // clean 且保留分支无数据可丢，直接执行；其余先过行内确认条
    if (t.dirty > 0 || withBranch) {
      setPendingRemove(r);
    } else {
      void doRemove(r);
    }
  };

  const switchTo = (t: Row) =>
    switchAction.run(() => workspaceSwitch(t.path), {
      okText: `已切换到 ${t.branch}`,
      errPrefix: "切换失败",
      // 成功后才置勾标：失败乐观 setActive 会把活跃标记画在没切成的树上
      onOk: () => setActive(t.path),
    });

  return (
    <div class="border-b border-[var(--border)] px-3 py-3">
      <div class="text-2xs uppercase tracking-wider text-[var(--text-faint)] mb-2 flex items-center gap-1.5">
        <GitBranch size={11} class="text-[var(--text-faint)]" />
        worktree 并行看板
      </div>
      <Show when={pendingRemove()}>
        {(r) => (
          <div class="mb-2 rounded-lg border border-[var(--warn)]/50 bg-[var(--warn)]/5 px-3 py-2.5 text-xs space-y-2">
            <div class="text-[var(--warn)]">{confirmText(r())}</div>
            <div class="flex gap-2">
              <button
                class="pressable px-2.5 py-1 rounded text-2xs bg-[var(--accent)] text-[var(--accent-contrast)] disabled:opacity-50"
                disabled={removeAction.pending()}
                onClick={() => {
                  const p = r();
                  setPendingRemove(null);
                  void doRemove(p);
                }}
              >
                确认删除
              </button>
              <button
                class="pressable px-2.5 py-1 rounded text-2xs border border-[var(--border)] text-[var(--text-dim)]"
                onClick={() => setPendingRemove(null)}
              >
                取消
              </button>
            </div>
          </div>
        )}
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
                  class="opacity-0 group-hover:opacity-100 pressable px-1 rounded text-2xs text-[var(--text-faint)] hover:text-[var(--text)] disabled:opacity-50"
                  title="切换工作区到此树（会话跑在该隔离目录）"
                  disabled={switchAction.pending()}
                  onClick={() => void switchTo(t)}
                >
                  切换
                </button>
              </Show>
              <button
                class="opacity-0 group-hover:opacity-100 pressable px-1 rounded text-[var(--text-faint)] hover:text-[var(--text)] disabled:opacity-50"
                title={
                  t.path === active()
                    ? "当前活跃 worktree 不可删除（先切换到其他目录）"
                    : "移除 worktree（分支保留）"
                }
                disabled={t.path === active() || removeAction.pending()}
                onClick={() => requestRemove(t, false)}
              >
                <Trash2 size={11} />
              </button>
              <button
                class="opacity-0 group-hover:opacity-100 pressable px-1 rounded text-2xs text-[var(--err)] disabled:opacity-50"
                title={
                  t.path === active()
                    ? "当前活跃 worktree 不可删除（先切换到其他目录）"
                    : "移除并删除分支"
                }
                disabled={t.path === active() || removeAction.pending()}
                onClick={() => requestRemove(t, true)}
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
