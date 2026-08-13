import { A } from "@solidjs/router";
import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { Bot, Folders, Moon, Plus, Search, Settings as SettingsIcon, Sun } from "lucide-solid";
import SessionTree from "./SessionTree";
import { initSessions, mountSessionEvents, newSession } from "../lib/state";
import { onDragStart } from "../lib/drag";
import { theme, toggleTheme } from "../lib/theme";
import { errText } from "./err-text";
import { isTauri } from "../lib/runtime";
import { openCommandPalette } from "../lib/command-palette";

const NAV_LINK_CLASS =
  "pressable flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-sm text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60 hover:text-[var(--text)]";

/** 左栏：品牌与搜索 + 一级导航 + 项目-会话树 + 底部设置。 */
export default function Sidebar() {
  // 首载失败与空侧栏区分（Session/Workspaces 同模式）：错误条 + 重试，不静默成空壳
  const [loadErr, setLoadErr] = createSignal("");
  const boot = async () => {
    try {
      await initSessions();
      setLoadErr("");
    } catch (e) {
      setLoadErr(errText(e));
    }
  };
  onMount(async () => {
    // run 存亡/resync 驱动会话列表刷新（running 圆点），随 Sidebar 生命周期注销
    onCleanup(mountSessionEvents());
    await boot();
  });

  return (
    <aside
      aria-label="应用侧边栏"
      class="shrink-0 flex flex-col border-r border-[var(--border)] bg-[var(--bg-raised)]"
      style={{ width: "var(--sidebar-w)" }}
    >
      {/* 红绿灯占位条只在 Tauri 无边框窗口下需要：浏览器/原生标题栏下是纯浪费 */}
      <Show when={isTauri()}>
        <div class="traffic-pad" data-tauri-drag-region onMouseDown={onDragStart} />
      </Show>
      <div class="px-3 pb-2">
        <A
          href="/"
          class="block px-1 text-lg font-semibold tracking-tight text-[var(--accent-hover)]"
          aria-label="Kxen 首页"
        >
          kxen
        </A>
      </div>
      <div class="px-2.5 pb-2">
        <button
          class="pressable flex w-full items-center gap-2 rounded-md border border-[var(--border)] bg-[var(--bg)]/40 px-2.5 py-1.5 text-left text-sm text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60 hover:text-[var(--text)]"
          onClick={openCommandPalette}
          aria-keyshortcuts="Meta+K Control+K"
        >
          <Search size={14} />
          <span class="flex-1">搜索</span>
          <kbd class="text-2xs text-[var(--text-faint)]">Cmd K</kbd>
        </button>
      </div>
      <nav class="px-2.5 pb-2 space-y-0.5" aria-label="主要导航">
        <A
          href="/workspaces"
          class={NAV_LINK_CLASS}
          activeClass="bg-[var(--bg-overlay)] text-[var(--text)]"
        >
          <Folders size={14} />
          工作区
        </A>
        <A
          href="/bots"
          class={NAV_LINK_CLASS}
          activeClass="bg-[var(--bg-overlay)] text-[var(--text)]"
        >
          <Bot size={14} />
          Bots
        </A>
      </nav>
      <Show when={loadErr()}>
        {(err) => (
          <div class="mx-3 mb-2 rounded-md border border-[var(--err)]/50 bg-[var(--err)]/5 px-2.5 py-2 space-y-1.5">
            <div class="text-2xs text-[var(--err)]">加载会话列表失败：{err()}</div>
            <button
              class="pressable px-2 py-0.5 rounded border border-[var(--border)] text-2xs text-[var(--text-dim)]"
              onClick={() => void boot()}
            >
              重试
            </button>
          </div>
        )}
      </Show>
      <div class="min-h-0 flex-1 flex flex-col border-t border-[var(--border)] pt-2">
        <div class="flex items-center gap-2 px-3 pb-1">
          <span class="flex-1 text-2xs font-medium uppercase tracking-wider text-[var(--text-faint)]">
            项目
          </span>
          <button
            class="pressable rounded p-1 text-[var(--text-faint)] hover:bg-[var(--bg-overlay)] hover:text-[var(--text)]"
            title="新建会话"
            aria-label="新建会话"
            onClick={() => void newSession()}
          >
            <Plus size={13} />
          </button>
        </div>
        <SessionTree />
      </div>
      <div class="border-t border-[var(--border)] p-2.5">
        <div class="flex items-center gap-1">
          <A
            href="/settings"
            class={`${NAV_LINK_CLASS} flex-1`}
            activeClass="bg-[var(--bg-overlay)] text-[var(--text)]"
          >
            <SettingsIcon size={14} />
            设置
          </A>
          <button
            class="pressable rounded-md p-2 text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60 hover:text-[var(--text)]"
            title="切换明暗主题"
            aria-label="切换明暗主题"
            onClick={(e) => toggleTheme(e.clientX, e.clientY)}
          >
            {theme() === "dark" ? <Moon size={14} /> : <Sun size={14} />}
          </button>
        </div>
      </div>
    </aside>
  );
}
