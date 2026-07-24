import { For, Show } from "solid-js";
import { X } from "lucide-solid";
import {
  activeAgentFocus,
  activeSessionId,
  agents,
  isMainFocus,
  setActiveAgentFocus,
} from "../lib/state";
import { agentsStop } from "../lib/team";
import { statusDot } from "../lib/variants";
import { KIND_BADGE, STATUS_TEXT, STATUS_TONE } from "../lib/agent-display";
import { onDragStart } from "../lib/drag";

/** Content 顶栏：主会话与每个 agent run 平级成一等 tab，点击切换 PrimaryContent。
 *  Main 固定在横向滚动区外，agent chip 超宽滚动不挤 Main。
 *  running（working/idle）chip hover 出停止按钮，经 agents.stop RPC 按名停 run。 */
export default function TopAgentBar() {
  const stopAgent = async (name: string) => {
    const sid = activeSessionId();
    if (!sid) return;
    await agentsStop(sid, name).catch(() => false);
    // 停的是当前选中 chip 才切回 main：停后台 run 不得抢走用户正在看的窗格
    if (activeAgentFocus() === name) setActiveAgentFocus("main");
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
              tone={STATUS_TONE[a.status] ?? { tone: "faint", pulse: false }}
              title={`${KIND_BADGE[a.kind] ?? a.kind} · ${STATUS_TEXT[a.status] ?? a.status} · ${a.model.model}`}
              onClick={() => setActiveAgentFocus(a.name)}
              onStop={
                a.status === "working" || a.status === "idle"
                  ? () => void stopAgent(a.name)
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
  tone?: { tone: "ok" | "accent" | "err" | "faint"; pulse: boolean };
  title: string;
  onClick: () => void;
  onStop?: (() => void) | undefined;
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
        }}
        title={props.title}
        onClick={props.onClick}
      >
        <Show when={props.tone}>{(t) => <span class={statusDot(t())} />}</Show>
        <span class="max-w-32 truncate">{props.label}</span>
        <Show when={props.sub}>
          <span class="text-2xs text-[var(--text-faint)] max-w-24 truncate">{props.sub}</span>
        </Show>
      </button>
      <Show when={props.onStop}>
        <button
          data-stop
          class="hidden group-hover:flex absolute right-1 top-1/2 -translate-y-1/2 items-center justify-center w-3.5 h-3.5 rounded bg-[var(--bg-overlay)] text-[var(--text-faint)] hover:text-[var(--err)]"
          title={`停止 ${props.label}`}
          onClick={() => props.onStop?.()}
        >
          <X size={10} />
        </button>
      </Show>
    </div>
  );
}
