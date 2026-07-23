// 中心看板：多 workspace 卡片视图（会话数/运行态/最近活动/脏文件数），点击切换并回会话页。
import { createSignal, For, onMount, Show } from "solid-js";
import { A, useNavigate } from "@solidjs/router";
import { ArrowLeft, FolderGit2, Play } from "lucide-solid";
import { workspacesOverview, workspaceSwitch, type WorkspaceOverview } from "../lib/chat";
import { relTime } from "../lib/time";
import { onDragStart } from "../lib/drag";

export default function Workspaces() {
  const navigate = useNavigate();
  const [cards, setCards] = createSignal<WorkspaceOverview[]>([]);
  const [note, setNote] = createSignal("");

  onMount(async () => {
    const list = await workspacesOverview().catch(() => null);
    if (list) setCards(list);
  });

  const open = async (path: string) => {
    try {
      await workspaceSwitch(path);
      navigate("/");
    } catch (e) {
      setNote(String(e));
      setTimeout(() => setNote(""), 3000);
    }
  };

  const basename = (p: string) => p.split("/").filter(Boolean).pop() ?? p;

  return (
    <div class="h-full flex-1 overflow-auto">
      <div class="h-8" data-tauri-drag-region onMouseDown={onDragStart} />
      <div class="px-8 py-6 pt-2">
        <A
          href="/"
          class="inline-flex items-center gap-1.5 text-xs text-[var(--text-dim)] hover:text-[var(--text)] mb-4"
        >
          <ArrowLeft size={13} />
          返回会话
        </A>
        <h1 class="text-lg font-medium text-[var(--text)] mb-4">工作区</h1>
        <Show when={note()}>
          <div class="text-xs text-[var(--err,#e5534b)] mb-3">{note()}</div>
        </Show>
        <Show
          when={cards().length > 0}
          fallback={<div class="text-sm text-[var(--text-dim)]">还没有工作区记录</div>}
        >
          <div class="grid grid-cols-2 gap-3 max-w-3xl">
            <For each={cards()}>
              {(c) => (
                <button
                  class="pressable text-left rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-4 hover:border-[var(--text-dim)] transition-colors"
                  onClick={() => void open(c.path)}
                >
                  <div class="flex items-center gap-2 mb-1">
                    <FolderGit2 size={15} class="text-[var(--text-dim)] shrink-0" />
                    <span class="text-sm font-medium text-[var(--text)] truncate">
                      {basename(c.path)}
                    </span>
                    <Show when={c.running > 0}>
                      <span class="ml-auto inline-flex items-center gap-1 text-xs text-[var(--ok)] shrink-0">
                        <Play size={11} />
                        {c.running} 运行中
                      </span>
                    </Show>
                  </div>
                  <div class="text-xs text-[var(--text-dim)] truncate mb-2 selectable">
                    {c.path}
                  </div>
                  <div class="flex items-center gap-3 text-xs text-[var(--text-dim)]">
                    <span>{c.sessions} 会话</span>
                    <Show when={c.dirty !== null}>
                      <span classList={{ "text-[var(--warn,#d29922)]": (c.dirty ?? 0) > 0 }}>
                        {c.dirty} 未提交
                      </span>
                    </Show>
                    <span class="ml-auto">{relTime(c.last_activity)}</span>
                  </div>
                </button>
              )}
            </For>
          </div>
        </Show>
      </div>
    </div>
  );
}
