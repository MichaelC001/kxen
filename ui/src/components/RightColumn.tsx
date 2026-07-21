import { createEffect, createSignal, For, Show, onCleanup, onMount } from "solid-js";
import { Bot, ChevronRight, X } from "lucide-solid";
import { agentsTranscript, onTopic, teamMessage, type TranscriptEntry } from "../lib/chat";
import { statusDot } from "../lib/variants";
import { activeSessionId, agents, focusAgent, refreshAgents, setFocusAgent } from "../lib/state";
import Dock from "./Dock";

const STATUS_TONE: Record<string, { tone: "ok" | "accent" | "err" | "faint"; pulse: boolean }> = {
  working: { tone: "ok", pulse: true },
  idle: { tone: "faint", pulse: false },
  done: { tone: "accent", pulse: false },
  failed: { tone: "err", pulse: false },
  shutdown: { tone: "faint", pulse: false },
};

const STATUS_TEXT: Record<string, string> = {
  working: "工作中",
  idle: "空闲",
  done: "已完成",
  failed: "失败",
  shutdown: "已关闭",
};

const KIND_BADGE: Record<string, string> = {
  teammate: "team",
  subagent: "sub",
  workflow: "flow",
};

/** 右列：上 = 子代理窗格（≤3 垂直，>3 tabs）；下 = focus 详情 | 主会话上下文。 */
export default function RightColumn() {
  let timer: ReturnType<typeof setInterval> | undefined;
  onMount(async () => {
    await refreshAgents();
    timer = setInterval(() => void refreshAgents(), 3000);
  });
  onCleanup(() => timer && clearInterval(timer));

  const [tab, setTab] = createSignal(0);
  const panes = () => agents();
  const showTabs = () => panes().length > 3;
  const visiblePanes = () => (showTabs() ? panes().slice(tab(), tab() + 1) : panes().slice(0, 3));

  return (
    <div class="w-full h-full flex flex-col bg-[var(--bg-raised)]">
      {/* 子代理窗格区 */}
      <Show when={panes().length > 0}>
        <div class="shrink-0 border-b border-[var(--border)]" style={{ "max-height": "45%" }}>
          <Show when={showTabs()}>
            <div class="flex items-center gap-0.5 px-2 pt-1.5 text-2xs">
              <For each={panes()}>
                {(a, i) => (
                  <button
                    class="px-1.5 py-0.5 rounded"
                    classList={{
                      "bg-[var(--bg-overlay)] text-[var(--text)]": tab() === i(),
                      "text-[var(--text-faint)]": tab() !== i(),
                    }}
                    onClick={() => setTab(i())}
                  >
                    {a.name}
                  </button>
                )}
              </For>
            </div>
          </Show>
          <div class="overflow-y-auto">
            <For each={visiblePanes()}>{(a) => <AgentPane name={a.name} />}</For>
          </div>
        </div>
      </Show>

      {/* focus 详情 | 主会话上下文 */}
      <div class="flex-1 min-h-0">
        <Show when={focusAgent()} fallback={<Dock />}>
          {(name) => <FocusView name={name()} />}
        </Show>
      </div>
    </div>
  );
}

/** 单个子代理窗格：状态行 + 最近输出预览，点击 focus。 */
function AgentPane(props: { name: string }) {
  const activity = () => agents().find((a) => a.name === props.name);
  const [preview, setPreview] = createSignal("");
  let unlisten: (() => void) | undefined;

  onMount(async () => {
    const t = await agentsTranscript(activeSessionId(), props.name).catch(
      () => [] as TranscriptEntry[],
    );
    const lastText = [...t].reverse().find((e) => e.kind === "text" && e.text);
    if (lastText?.text) setPreview(lastText.text.slice(-120));
    unlisten = await onTopic(["llm.delta"], (_topic, payload) => {
      const p = payload as { agent?: string; session_id?: string; kind?: string; text?: string };
      if (p.agent !== props.name || p.session_id !== activeSessionId()) return;
      if (p.kind === "text" && p.text) {
        setPreview((prev) => (prev + p.text).slice(-120));
      }
    });
  });
  onCleanup(() => unlisten?.());

  return (
    <button
      class="w-full text-left px-3 py-2 border-b border-[var(--border)]/50 hover:bg-[var(--bg-overlay)]/40"
      onClick={() => setFocusAgent(props.name)}
    >
      <div class="flex items-center gap-1.5">
        <span
          class={statusDot(
            STATUS_TONE[activity()?.status ?? "idle"] ?? { tone: "faint", pulse: false },
          )}
        />
        <span class="text-xs font-medium">{props.name}</span>
        <span class="text-2xs px-1 rounded border border-[var(--border)] text-[var(--text-faint)]">
          {KIND_BADGE[activity()?.kind ?? "subagent"]}
        </span>
        <span class="text-2xs text-[var(--text-faint)] truncate">{activity()?.model.model}</span>
        <span class="text-2xs text-[var(--text-faint)] ml-auto">
          {STATUS_TEXT[activity()?.status ?? "idle"]}
        </span>
        <ChevronRight size={11} class="text-[var(--text-faint)]" />
      </div>
      <Show when={preview()}>
        <div class="text-2xs text-[var(--text-faint)] truncate mt-1 font-mono">{preview()}</div>
      </Show>
    </button>
  );
}

/** focus 代理详情：状态头 + 转录 + （teammate 可对话输入）。 */
function FocusView(props: { name: string }) {
  const [entries, setEntries] = createSignal<TranscriptEntry[]>([]);
  const [draft, setDraft] = createSignal("");
  let unlisten: (() => void) | undefined;
  let listRef: HTMLDivElement | undefined;

  const activity = () => agents().find((a) => a.name === props.name);
  const scroll = () => queueMicrotask(() => listRef && (listRef.scrollTop = listRef.scrollHeight));

  createEffect(() => {
    const name = props.name;
    void agentsTranscript(activeSessionId(), name).then((t) => {
      setEntries(t);
      scroll();
    });
  });

  onMount(async () => {
    unlisten = await onTopic(["llm.delta"], (_topic, payload) => {
      const p = payload as TranscriptEntry & { agent?: string; session_id?: string };
      if (p.agent !== props.name || p.session_id !== activeSessionId()) return;
      setEntries((prev) => {
        const last = prev.at(-1);
        if (p.kind === "text" && last?.kind === "text") {
          return [...prev.slice(0, -1), { ...last, text: (last.text ?? "") + (p.text ?? "") }];
        }
        return [...prev.slice(-199), p];
      });
      scroll();
    });
  });
  onCleanup(() => unlisten?.());

  const send = async () => {
    const text = draft().trim();
    if (!text) return;
    setDraft("");
    await teamMessage(activeSessionId(), props.name, text);
  };

  return (
    <div class="h-full flex flex-col">
      <div class="shrink-0 px-3 py-2 border-b border-[var(--border)] flex items-center gap-1.5">
        <button
          class="pressable p-0.5 rounded text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60"
          title="取消 focus"
          onClick={() => setFocusAgent(null)}
        >
          <X size={12} />
        </button>
        <Bot size={13} class="text-[var(--accent-hover)]" />
        <span class="text-xs font-medium">{props.name}</span>
        <span class="text-2xs px-1 rounded border border-[var(--border)] text-[var(--text-faint)]">
          {KIND_BADGE[activity()?.kind ?? "subagent"]}
        </span>
        <span class="text-2xs text-[var(--text-faint)]">{activity()?.model.model}</span>
        <span class="text-2xs text-[var(--text-faint)] ml-auto">
          {STATUS_TEXT[activity()?.status ?? "idle"]}
        </span>
      </div>
      <div ref={(el) => (listRef = el)} class="flex-1 overflow-auto px-3 py-2 space-y-1.5">
        <For each={entries()}>
          {(e) => {
            if (e.kind === "tool_call" || e.kind === "tool_result") {
              return (
                <div class="text-2xs font-mono text-[var(--text-faint)] truncate">{`${e.name}: ${e.summary ?? ""}`}</div>
              );
            }
            if (e.kind === "error") {
              return <div class="text-2xs text-[var(--err)]">{e.message}</div>;
            }
            if (e.kind === "text" || e.kind === "reasoning") {
              return (
                <div
                  class="text-xs whitespace-pre-wrap"
                  classList={{ "text-[var(--text-faint)]": e.kind === "reasoning" }}
                >
                  {e.text}
                </div>
              );
            }
            return null;
          }}
        </For>
        <Show when={entries().length === 0}>
          <div class="text-2xs text-[var(--text-faint)]">等待输出…</div>
        </Show>
      </div>
      <Show
        when={
          activity()?.kind === "teammate" && ["working", "idle"].includes(activity()?.status ?? "")
        }
      >
        <div class="shrink-0 p-2 border-t border-[var(--border)]">
          <input
            class="w-full bg-transparent border border-[var(--border)] rounded px-2 py-1 text-xs"
            placeholder={`对 ${props.name} 说话…`}
            value={draft()}
            onInput={(e) => setDraft(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void send();
            }}
          />
        </div>
      </Show>
    </div>
  );
}
