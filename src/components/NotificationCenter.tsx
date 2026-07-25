// 通知中心：铃铛 + 未读计数 + 下拉面板（时间/文本/清空）。未读基线存 localStorage。
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { Bell, Trash2 } from "lucide-solid";
import EmptyLine from "./EmptyLine";
import { client } from "../lib/client";
import { onClickOutside } from "../lib/dismiss";
import { relTime } from "../lib/time";

interface Notice {
  at: number;
  text: string;
}

const READ_KEY = "kxen-notif-read-at";

export default function NotificationCenter() {
  const [open, setOpen] = createSignal(false);
  const [items, setItems] = createSignal<Notice[]>([]);
  let root: HTMLDivElement | undefined;
  onClickOutside(
    () => root,
    () => setOpen(false),
  );

  const readAt = () => Number(localStorage.getItem(READ_KEY) ?? 0);
  const unread = () => items().filter((n) => n.at > readAt()).length;

  const reload = async () => {
    const list = await client.rpc<Notice[]>("notifications.list").catch(() => []);
    setItems(list);
  };

  onMount(() => {
    void reload();
    const timer = setInterval(() => void reload(), 5000);
    return () => clearInterval(timer);
  });

  // bus lag 丢帧后服务端下发 resync：不等下一轮轮询，立即按真源重拉
  const offResync = client.onResync(() => void reload());
  onCleanup(offResync);

  const openPanel = () => {
    setOpen(!open());
    if (!open()) void reload();
  };

  const markRead = () => {
    localStorage.setItem(READ_KEY, String(Date.now()));
    setOpen(false);
  };

  const clearAll = async () => {
    await client.rpc("notifications.clear").catch(() => {});
    localStorage.setItem(READ_KEY, String(Date.now()));
    await reload();
  };

  return (
    <div class="relative" ref={(el) => (root = el)}>
      <button
        class="pressable relative px-1.5 py-1 rounded text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60"
        onClick={openPanel}
        title="通知中心"
      >
        <Bell size={13} />
        <Show when={unread() > 0}>
          <span class="absolute -top-0.5 -right-0.5 min-w-3.5 h-3.5 px-0.5 rounded-full bg-[var(--err)] text-white text-2xs leading-3.5 text-center">
            {unread() > 9 ? "9+" : unread()}
          </span>
        </Show>
      </button>
      <Show when={open()}>
        <div class="composer-popup absolute bottom-full left-0 mb-1.5 w-72 max-h-80 overflow-y-auto rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] z-30">
          <div class="flex items-center justify-between px-3 py-2 border-b border-[var(--border)]">
            <span class="text-xs text-[var(--text-dim)]">通知</span>
            <div class="flex gap-2">
              <button
                class="text-2xs text-[var(--text-faint)] hover:text-[var(--text)]"
                onClick={markRead}
              >
                全部已读
              </button>
              <button
                class="text-2xs text-[var(--text-faint)] hover:text-[var(--err)] flex items-center gap-0.5"
                onClick={() => void clearAll()}
              >
                <Trash2 size={10} />
                清空
              </button>
            </div>
          </div>
          <For each={items()} fallback={<EmptyLine text="暂无通知" />}>
            {(n) => (
              <div class="px-3 py-2 border-b border-[var(--border)] last:border-0">
                <div class="text-2xs text-[var(--text-faint)]">{relTime(n.at)}</div>
                <div class="text-xs leading-snug break-words">{n.text}</div>
              </div>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}
