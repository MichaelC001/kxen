// SessionTree：Codex 式项目-会话树（每组 ≤5 条，组可折叠，行内置顶/重命名/删除确认/拖拽排序）。
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { ChevronDown, ChevronRight, FolderOpen, FolderPlus, PenLine, Plus } from "lucide-solid";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  sessionDelete,
  sessionUpdateMeta,
  workspaceAdd,
  workspaceList,
  workspaceSwitch,
  type SessionMeta,
  type Workspace,
} from "../lib/chat";
import { newSession, refreshSessions, sessions, switchSession } from "../lib/state";
import { sortGroup } from "../lib/order";
import SessionRow from "./SessionRow";
import EmptyLine from "./EmptyLine";

const MAX_PER_GROUP = 5;

interface Group {
  path: string;
  name: string;
  sessions: SessionMeta[];
}

export default function SessionTree() {
  const [recents, setRecents] = createSignal<Workspace[]>([]);
  const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());
  const [expanded, setExpanded] = createSignal<Set<string>>(new Set());
  const [adding, setAdding] = createSignal(false);
  const [newPath, setNewPath] = createSignal("");
  let dragId = "";

  const reloadRecents = async () => setRecents(await workspaceList().catch(() => []));

  onMount(() => {
    void reloadRecents();
    const timer = setInterval(() => void reloadRecents(), 10_000);
    onCleanup(() => clearInterval(timer));
  });

  const groups = (): Group[] => {
    const byDir = new Map<string, SessionMeta[]>();
    for (const s of sessions()) {
      const list = byDir.get(s.directory) ?? [];
      list.push(s);
      byDir.set(s.directory, list);
    }
    // 有会话的目录按最近会话排序，无会话的 recents 尾部跟上
    const dirs = [...byDir.keys()].sort((a, b) => {
      const ta = Math.max(...byDir.get(a)!.map((s) => s.updated_at));
      const tb = Math.max(...byDir.get(b)!.map((s) => s.updated_at));
      return tb - ta;
    });
    const out: Group[] = dirs.map((d) => ({
      path: d,
      name: d.split("/").filter(Boolean).pop() ?? d,
      sessions: sortGroup(byDir.get(d)!),
    }));
    for (const w of recents()) {
      if (!byDir.has(w.path)) {
        out.push({
          path: w.path,
          name: w.path.split("/").filter(Boolean).pop() ?? w.path,
          sessions: [],
        });
      }
    }
    return out;
  };

  const toggle = (path: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const toggleExpand = (path: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const open = async (path: string, id: string) => {
    await workspaceSwitch(path).catch(() => {});
    switchSession(id);
  };

  const quickNew = async (path: string) => {
    await workspaceSwitch(path).catch(() => {});
    await newSession();
  };

  const remove = async (id: string) => {
    await sessionDelete(id);
    await refreshSessions();
  };

  const addAndSwitch = async (path: string) => {
    await workspaceAdd(path).catch(() => {});
    await workspaceSwitch(path).catch(() => {});
    await refreshSessions();
    await reloadRecents();
  };

  // 原生目录选择器：用户不应手敲绝对路径
  const pickDir = async () => {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: "选择项目目录",
    }).catch(() => null);
    if (typeof selected === "string" && selected) await addAndSwitch(selected);
  };

  const addPath = async () => {
    const path = newPath().trim();
    if (!path) return;
    await addAndSwitch(path);
    setAdding(false);
    setNewPath("");
  };

  /** 拖拽排序：落点行的位置即为新序号，整组重写 sort_order 持久化。 */
  const dropOn = async (group: Group, targetId: string) => {
    if (!dragId || dragId === targetId) return;
    const list = group.sessions.filter((s) => !s.pinned);
    const from = list.findIndex((s) => s.id === dragId);
    const to = list.findIndex((s) => s.id === targetId);
    if (from < 0 || to < 0) return;
    const moved = list.splice(from, 1)[0]!;
    list.splice(to, 0, moved);
    for (let i = 0; i < list.length; i++) {
      await sessionUpdateMeta(list[i]!.id, { sort_order: i + 1 }).catch(() => {});
    }
    dragId = "";
    await refreshSessions();
  };

  return (
    <div class="flex-1 overflow-y-auto px-2 space-y-1">
      <For each={groups()}>
        {(group) => {
          const isCollapsed = () => collapsed().has(group.path);
          const visible = () =>
            expanded().has(group.path) ? group.sessions : group.sessions.slice(0, MAX_PER_GROUP);
          return (
            <div>
              <button
                class="group w-full flex items-center gap-1 px-1.5 py-1 rounded text-xs text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60"
                onClick={() => toggle(group.path)}
              >
                <Show when={isCollapsed()} fallback={<ChevronDown size={11} />}>
                  <ChevronRight size={11} />
                </Show>
                <FolderOpen size={12} class="text-[var(--accent-hover)]" />
                <span class="flex-1 text-left truncate font-medium" title={group.path}>
                  {group.name}
                </span>
                <span
                  role="button"
                  tabindex="0"
                  class="opacity-0 group-hover:opacity-100 px-0.5 rounded hover:text-[var(--text)]"
                  title="在此项目下新建会话"
                  onClick={(e) => {
                    e.stopPropagation();
                    void quickNew(group.path);
                  }}
                >
                  <Plus size={12} />
                </span>
                <Show when={group.sessions.length > 0}>
                  <span class="text-2xs text-[var(--text-faint)]">{group.sessions.length}</span>
                </Show>
              </button>
              <Show when={!isCollapsed()}>
                <div class="ml-4 space-y-0.5">
                  <For each={visible()}>
                    {(s) => (
                      <SessionRow
                        session={s}
                        onOpen={() => void open(group.path, s.id)}
                        onDelete={() => void remove(s.id)}
                        onChanged={() => void refreshSessions()}
                        draggable
                        onDragStart={() => (dragId = s.id)}
                        onDragOver={(e) => e.preventDefault()}
                        onDrop={() => void dropOn(group, s.id)}
                      />
                    )}
                  </For>
                  <Show when={group.sessions.length > MAX_PER_GROUP}>
                    <button
                      class="px-2 py-0.5 text-2xs text-[var(--text-faint)] hover:text-[var(--text-dim)]"
                      onClick={() => toggleExpand(group.path)}
                    >
                      {expanded().has(group.path)
                        ? "收起"
                        : `展开全部 ${group.sessions.length} 个…`}
                    </button>
                  </Show>
                  <Show when={group.sessions.length === 0}>
                    <EmptyLine text="无会话" />
                  </Show>
                </div>
              </Show>
            </div>
          );
        }}
      </For>
      <Show
        when={adding()}
        fallback={
          <div class="flex items-center gap-0.5">
            <button
              class="flex-1 flex items-center gap-1.5 px-1.5 py-1 rounded text-xs text-[var(--text-faint)] hover:bg-[var(--bg-overlay)]/60"
              onClick={() => void pickDir()}
            >
              <FolderPlus size={12} />
              添加项目目录…
            </button>
            <button
              class="p-1 rounded text-[var(--text-faint)] hover:bg-[var(--bg-overlay)]/60"
              title="手动输入路径"
              onClick={() => setAdding(true)}
            >
              <PenLine size={12} />
            </button>
          </div>
        }
      >
        <div class="flex items-center gap-1 px-1.5 py-1">
          <input
            class="flex-1 bg-transparent text-xs font-mono focus:outline-none placeholder:text-[var(--text-faint)]"
            placeholder="/绝对/路径"
            value={newPath()}
            onInput={(e) => setNewPath(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void addPath();
              if (e.key === "Escape") setAdding(false);
            }}
          />
          <button
            class="text-2xs px-1.5 py-0.5 rounded bg-[var(--accent)] text-[var(--accent-contrast)]"
            onClick={() => void addPath()}
          >
            添加
          </button>
        </div>
      </Show>
    </div>
  );
}
