import { createEffect, createSignal, For, Show } from "solid-js";
import { X } from "lucide-solid";
import {
  activeAgentFocus,
  activeSessionId,
  agents,
  isMainFocus,
  refreshAgents,
  setActiveAgentFocus,
} from "../lib/state";
import { agentsDismiss, agentsStop } from "../lib/team";
import { statusDot } from "../lib/variants";
import { kindBadge, statusText, statusTone } from "../lib/agent-display";
import { flashErr } from "../lib/flash";
import { formatError } from "../lib/error-text";
import { onDragStart } from "../lib/drag";

/** Content 顶栏：主会话与每个 agent run 平级成一等 tab，点击切换 PrimaryContent。
 *  Main 固定在横向滚动区外，agent chip 超宽滚动不挤 Main。
 *  running（working/idle）chip hover 出停止按钮（agents.stop 按名停 run）；
 *  终态（done/failed/shutdown）chip hover 出关闭按钮（agents.dismiss 移出名单）。 */
export default function TopAgentBar() {
  /** 乐观置灰：点击停止立即禁用 chip（防连点），成功靠轮询收敛、失败就地还原。 */
  const [stopping, setStopping] = createSignal("");

  // 轮询收敛口：目标 agent 不再是 running 态（或已从名单消失）即摘灰
  createEffect(() => {
    const name = stopping();
    if (!name) return;
    const a = agents().find((x) => x.name === name);
    if (!a || (a.status !== "working" && a.status !== "idle")) setStopping("");
  });

  const stopAgent = async (name: string) => {
    const sid = activeSessionId();
    if (!sid) return;
    setStopping(name);
    try {
      const ok = await agentsStop(sid, name);
      if (!ok) {
        flashErr(`停止 ${name} 失败：run 不存在或已关闭`);
        setStopping("");
        return;
      }
      // 停的是当前选中 chip 才切回 main：停后台 run 不得抢走用户正在看的窗格
      if (activeAgentFocus() === name) setActiveAgentFocus("main");
    } catch (e) {
      flashErr(`停止 ${name} 失败：${formatError(e instanceof Error ? e.message : String(e))}`);
      setStopping("");
    }
  };

  const dismissAgent = async (name: string) => {
    const sid = activeSessionId();
    if (!sid) return;
    try {
      const ok = await agentsDismiss(sid, name);
      if (!ok) {
        flashErr(`关闭 ${name} 失败：run 不存在或仍在运行`);
        return;
      }
      // 关的是当前选中 chip 才切回 main（同 stop 的窗格保护）
      if (activeAgentFocus() === name) setActiveAgentFocus("main");
      // 立即收敛名单，不等 3s 轮询
      await refreshAgents();
    } catch (e) {
      flashErr(`关闭 ${name} 失败：${formatError(e instanceof Error ? e.message : String(e))}`);
    }
  };

  return (
    <div
      class="material shrink-0 flex items-stretch px-1 border-b border-[var(--border)]"
      data-tauri-drag-region
      onMouseDown={onDragStart}
    >
      <Chip
        selected={isMainFocus()}
        label="Main"
        title="主会话"
        onClick={() => setActiveAgentFocus("main")}
      />
      <div class="flex-1 min-w-0 overflow-x-auto flex items-stretch">
        <For each={agents()}>
          {(a) => (
            <Chip
              selected={activeAgentFocus() === a.name}
              label={a.name}
              sub={a.model.model}
              tone={statusTone(a.status)}
              title={`${kindBadge(a.kind)} · ${statusText(a.status)} · ${a.model.model}`}
              stopping={stopping() === a.name}
              onClick={() => setActiveAgentFocus(a.name)}
              onStop={
                a.status === "working" || a.status === "idle"
                  ? () => void stopAgent(a.name)
                  : undefined
              }
              onDismiss={
                a.status === "done" || a.status === "failed" || a.status === "shutdown"
                  ? () => void dismissAgent(a.name)
                  : undefined
              }
            />
          )}
        </For>
      </div>
    </div>
  );
}

function Chip(props: {
  selected: boolean;
  label: string;
  sub?: string;
  tone?: { tone: "ok" | "warn" | "accent" | "err" | "faint"; pulse: boolean };
  title: string;
  onClick: () => void;
  onStop?: (() => void) | undefined;
  onDismiss?: (() => void) | undefined;
  stopping?: boolean;
}) {
  return (
    <div class="group relative shrink-0 flex items-stretch">
      <button
        data-chip
        class="pressable shrink-0 flex items-center gap-1.5 mx-0.5 my-1 px-2 py-1 rounded text-xs border"
        classList={{
          "bg-[var(--bg-overlay)] text-[var(--text)] border-[var(--border)]": props.selected,
          "border-transparent text-[var(--text-faint)] hover:text-[var(--text-dim)]":
            !props.selected,
          "opacity-50": props.stopping ?? false,
        }}
        disabled={props.stopping ?? false}
        title={props.title}
        onClick={props.onClick}
      >
        <Show when={props.tone}>{(t) => <span class={statusDot(t())} />}</Show>
        <span class="max-w-32 truncate">{props.label}</span>
        <Show when={props.sub}>
          <span class="text-2xs text-[var(--text-faint)] max-w-24 truncate">{props.sub}</span>
        </Show>
      </button>
      <Show when={props.onStop && !props.stopping}>
        <button
          data-stop
          class="hidden group-hover:flex absolute right-1 top-1/2 -translate-y-1/2 items-center justify-center w-3.5 h-3.5 rounded bg-[var(--bg-overlay)] text-[var(--text-faint)] hover:text-[var(--err)]"
          title={`停止 ${props.label}`}
          onClick={() => props.onStop?.()}
        >
          <X size={10} />
        </button>
      </Show>
      <Show when={props.onDismiss}>
        <button
          data-dismiss
          class="hidden group-hover:flex absolute right-1 top-1/2 -translate-y-1/2 items-center justify-center w-3.5 h-3.5 rounded bg-[var(--bg-overlay)] text-[var(--text-faint)] hover:text-[var(--text)]"
          title={`关闭 ${props.label}（移出名单）`}
          onClick={() => props.onDismiss?.()}
        >
          <X size={10} />
        </button>
      </Show>
    </div>
  );
}
