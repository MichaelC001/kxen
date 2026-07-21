// SessionTree：Codex 式项目-会话树（每组 ≤5 条，组可折叠，底部内嵌添加目录）。
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { ChevronDown, ChevronRight, FolderOpen, FolderPlus, X } from "lucide-solid";
import {
  sessionDelete,
  workspaceAdd,
  workspaceList,
  workspaceSwitch,
  type SessionMeta,
  type Workspace,
} from "../lib/chat";
import { activeSessionId, refreshSessions, sessions, switchSession } from "../lib/state";

const MAX_PER_GROUP = 5;

interface Group {
  path: string;
  name: string;
  sessions: SessionMeta[];
}

export default function SessionTree() {
  const [recents, setRecents] = createSignal<Workspace[]>([]);
  const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());
  const [adding, setAdding] = createSignal(false);
  const [newPath, setNewPath] = createSignal("");

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
      sessions: byDir.get(d)!,
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

  const open = async (path: string, id: string) => {
    await workspaceSwitch(path).catch(() => {});
    switchSession(id);
  };

  const remove = async (id: string) => {
    await sessionDelete(id);
    await refreshSessions();
  };

  const addPath = async () => {
    const path = newPath().trim();
    if (!path) return;
    await workspaceAdd(path).catch(() => {});
    await workspaceSwitch(path).catch(() => {});
    await refreshSessions();
    await reloadRecents();
    setAdding(false);
    setNewPath("");
  };

  return (
    <div class="flex-1 overflow-y-auto px-2 space-y-1">
      <For each={groups()}>
        {(group) => {
          const isCollapsed = () => collapsed().has(group.path);
          const visible = () => group.sessions.slice(0, MAX_PER_GROUP);
          return (
            <div>
              <button
                class="w-full flex items-center gap-1 px-1.5 py-1 rounded text-xs text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60"
                onClick={() => toggle(group.path)}
              >
                <Show when={isCollapsed()} fallback={<ChevronDown size={11} />}>
                  <ChevronRight size={11} />
                </Show>
                <FolderOpen size={12} class="text-[var(--accent-hover)]" />
                <span class="flex-1 text-left truncate font-medium" title={group.path}>
                  {group.name}
                </span>
                <Show when={group.sessions.length > 0}>
                  <span class="text-2xs text-[var(--text-faint)]">{group.sessions.length}</span>
                </Show>
              </button>
              <Show when={!isCollapsed()}>
                <div class="ml-4 space-y-0.5">
                  <For each={visible()}>
                    {(s) => (
                      <div
                        class="group flex items-center rounded-md text-sm cursor-pointer"
                        classList={{
                          "bg-[var(--bg-overlay)] text-[var(--text)]": s.id === activeSessionId(),
                          "text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60":
                            s.id !== activeSessionId(),
                        }}
                        onClick={() => void open(group.path, s.id)}
                      >
                        <span class="flex-1 px-2 py-1 truncate" title={s.title}>
                          {s.title}
                        </span>
                        <button
                          class="px-1.5 text-[var(--text-faint)] opacity-0 group-hover:opacity-100 hover:text-[var(--err)]"
                          title="删除会话"
                          onClick={(e) => {
                            e.stopPropagation();
                            void remove(s.id);
                          }}
                        >
                          <X size={12} />
                        </button>
                      </div>
                    )}
                  </For>
                  <Show when={group.sessions.length > MAX_PER_GROUP}>
                    <div class="px-2 py-0.5 text-2xs text-[var(--text-faint)]">
                      还有 {group.sessions.length - MAX_PER_GROUP} 个…
                    </div>
                  </Show>
                  <Show when={group.sessions.length === 0}>
                    <div class="px-2 py-0.5 text-2xs text-[var(--text-faint)]">无会话</div>
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
          <button
            class="w-full flex items-center gap-1.5 px-1.5 py-1 rounded text-xs text-[var(--text-faint)] hover:bg-[var(--bg-overlay)]/60"
            onClick={() => setAdding(true)}
          >
            <FolderPlus size={12} />
            添加项目目录…
          </button>
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
