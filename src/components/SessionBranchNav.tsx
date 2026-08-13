import { createMemo, For, Show } from "solid-js";
import { CornerUpLeft, FolderOpen, GitFork } from "lucide-solid";
import type { SessionMeta } from "../lib/chat";
import {
  buildSessionBranchRows,
  forkKindLabel,
  sessionBranchFamily,
} from "../lib/session-branches";

export default function SessionBranchNav(props: {
  current: () => SessionMeta | undefined;
  sessions: () => SessionMeta[];
  onSwitch: (id: string) => void;
}) {
  const family = createMemo(() => {
    const current = props.current();
    return current ? sessionBranchFamily(current, props.sessions()) : [];
  });
  const rows = createMemo(() => buildSessionBranchRows(family()));
  const currentIndex = () => rows().findIndex((row) => row.session.id === props.current()?.id);
  const parent = () => {
    const parentId = props.current()?.parent_id;
    return parentId ? props.sessions().find((session) => session.id === parentId) : undefined;
  };
  const detail = () => {
    const current = props.current();
    if (!current?.parent_id) return "分支族根会话；同族对话历史独立，Workspace 文件状态共享";
    const point = current.fork_point;
    const cursor = point
      ? `${point.position === "before" ? "消息前" : "消息后"}，父会话第 ${point.message_index} 条`
      : "存量分支，精确分叉点未记录";
    return `${forkKindLabel(current.fork_kind)}：${cursor}；对话历史独立，Workspace 文件状态共享`;
  };

  return (
    <Show when={props.current() && (rows().length > 1 || props.current()?.parent_id)}>
      <span
        class="inline-flex min-w-0 items-center gap-1 text-[var(--text-faint)]"
        title={detail()}
      >
        <GitFork size={12} class="shrink-0 text-[var(--accent-hover)]" />
        <Show when={props.current()?.parent_id}>
          <button
            class="pressable rounded px-1 py-0.5 hover:text-[var(--text)] disabled:opacity-40"
            disabled={!parent()}
            title={parent() ? `返回父分支：${parent()!.title}` : "父分支已删除，可从系统废纸篓恢复"}
            onClick={() => parent() && props.onSwitch(parent()!.id)}
          >
            <CornerUpLeft size={11} />
          </button>
        </Show>
        <select
          class="max-w-44 truncate rounded border border-[var(--border)] bg-[var(--bg-raised)] px-1 py-0.5 text-2xs text-[var(--text-dim)]"
          aria-label="切换对话分支"
          title={detail()}
          value={props.current()?.id ?? ""}
          onChange={(event) => props.onSwitch(event.currentTarget.value)}
        >
          <For each={rows()}>
            {(row) => (
              <option value={row.session.id}>
                {`${"  ".repeat(row.depth)}${row.parentMissing ? "[父分支已删除] " : ""}${row.session.title}`}
              </option>
            )}
          </For>
        </select>
        <span class="shrink-0 text-2xs">
          {currentIndex() + 1}/{rows().length}
        </span>
        <FolderOpen size={11} class="shrink-0" aria-label="Workspace 文件状态由分支共享" />
      </span>
    </Show>
  );
}
