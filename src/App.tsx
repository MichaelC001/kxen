import { Route, Router } from "@solidjs/router";
import Sidebar from "./components/Sidebar";
import RightColumn from "./components/RightColumn";
import StatusBar from "./components/StatusBar";
import CommandPalette from "./components/CommandPalette";
import ContextMenu from "./components/ContextMenu";
import FlashHost from "./components/FlashHost";
import TopAgentBar from "./components/TopAgentBar";
import AgentFocusView from "./components/AgentFocusView";
import Session from "./pages/Session";
import Settings from "./pages/Settings";
import Workspaces from "./pages/Workspaces";
import {
  activeAgentFocus,
  agents,
  hasConversation,
  isMainFocus,
  refreshAgents,
  setNavigator,
} from "./lib/state";
import { startAgentsPolling } from "./lib/agents-poll";
import { mountShortcuts } from "./lib/shortcuts";
import { openMenu } from "./lib/context-menu";
import { mountOsNotificationJump } from "./lib/os-notify";
import { useNavigate } from "@solidjs/router";
import { onCleanup, onMount, Show } from "solid-js";

function Home() {
  // agents 名单同时驱动 TopAgentBar 与 RightColumn，轮询上提到共同父级（原先 RightColumn 独占）
  onMount(async () => {
    await refreshAgents();
  });
  onCleanup(startAgentsPolling());

  return (
    <div class="flex-1 min-w-0 flex flex-col">
      {/* 顶栏常驻占位：空会话首屏只隐藏不卸载（visibility 保高），出现/消失不再挤动下方布局 */}
      <div classList={{ invisible: !hasConversation() && agents().length === 0 }}>
        <TopAgentBar />
      </div>
      <div class="flex-1 min-h-0 flex">
        {/* Session 常驻只切显隐：卸载会断流监听、丢滚动/草稿态（选中 agent 时主流仍在跑） */}
        <div
          class="flex-1 min-w-0 flex-col"
          classList={{ flex: isMainFocus(), hidden: !isMainFocus() }}
        >
          <Session />
        </div>
        <Show when={!isMainFocus()}>
          <AgentFocusView name={activeAgentFocus()} />
        </Show>
        {/* 右栏显隐与顶栏同条件：有 agent 无对话时不得顶栏有右栏无 */}
        <div
          class="dock-wrap"
          classList={{ "dock-hidden": !hasConversation() && agents().length === 0 }}
        >
          <RightColumn />
        </div>
      </div>
      <StatusBar />
    </div>
  );
}

function Layout(props: { children?: import("solid-js").JSX.Element }) {
  const navigate = useNavigate();
  setNavigator(navigate);
  let unmount: (() => void) | undefined;
  let unlistenOs: (() => void) | undefined;
  onMount(() => {
    unmount = mountShortcuts();
    window.addEventListener("contextmenu", onGlobalContextMenu);
    void mountOsNotificationJump()
      .then((u) => (unlistenOs = u))
      // 非 Tauri 环境（vitest / 纯浏览器 dev）无 event bridge：降级为无点击回跳
      .catch(() => {});
  });
  onCleanup(() => {
    unmount?.();
    unlistenOs?.();
    window.removeEventListener("contextmenu", onGlobalContextMenu);
  });
  return (
    <div class="h-screen flex overflow-hidden">
      <Sidebar />
      <main class="flex-1 min-w-0 flex">{props.children}</main>
      <CommandPalette />
      <ContextMenu />
      <FlashHost />
    </div>
  );
}

/** 全局右键：输入控件给编辑命令，可选区给复制，其余屏蔽 webview 默认（reload/inspect）。 */
function onGlobalContextMenu(e: MouseEvent) {
  const target = e.target as HTMLElement;
  if (target.closest("input, textarea, [contenteditable='true']")) {
    openMenu(e, [
      { label: "剪切", action: () => document.execCommand("cut") },
      { label: "复制", action: () => document.execCommand("copy") },
      {
        label: "粘贴",
        action: () =>
          void navigator.clipboard
            .readText()
            .then((t) => document.execCommand("insertText", false, t))
            .catch(() => {}),
      },
      { label: "全选", action: () => document.execCommand("selectAll") },
    ]);
    return;
  }
  if (target.closest(".selectable") && window.getSelection()?.toString()) {
    openMenu(e, [{ label: "复制", action: () => document.execCommand("copy") }]);
    return;
  }
  e.preventDefault();
}

export default function App() {
  return (
    <Router root={Layout}>
      <Route path="/" component={Home} />
      <Route path="/settings" component={Settings} />
      <Route path="/workspaces" component={Workspaces} />
    </Router>
  );
}
