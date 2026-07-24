import { createEffect, createSignal, For, Show, onCleanup, onMount } from "solid-js";
import { Bot, X } from "lucide-solid";
import { onTopic } from "../lib/chat";
import { agentsTranscript, teamMessage, type TranscriptEntry } from "../lib/team";
import { KIND_BADGE, STATUS_TEXT } from "../lib/agent-display";
import { formatError } from "../lib/error-text";
import { activeSessionId, agents, setActiveAgentFocus } from "../lib/state";

/** 选中 agent 时的 PrimaryContent：状态头 + 全量转录 +（teammate 可对话输入）。
 *  由 RightColumn 的窄栏 FocusView 迁出 —— 转录是主内容，不再塞进右栏。 */
export default function AgentFocusView(props: { name: string }) {
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
    <div class="h-full flex-1 min-w-0 flex flex-col">
      <div class="material shrink-0 px-4 py-2.5 border-b border-[var(--border)] flex items-center gap-1.5">
        <button
          class="pressable p-0.5 rounded text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60"
          title="回到主会话"
          onClick={() => setActiveAgentFocus("main")}
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
      <div ref={(el) => (listRef = el)} class="flex-1 overflow-auto px-4 py-3 space-y-1.5">
        <For each={entries()}>
          {(e) => {
            if (e.kind === "tool_call" || e.kind === "tool_result") {
              return (
                <div class="text-2xs font-mono text-[var(--text-faint)] truncate">{`${e.name}: ${e.summary ?? ""}`}</div>
              );
            }
            if (e.kind === "error") {
              return <div class="text-2xs text-[var(--err)]">{formatError(e.message ?? "")}</div>;
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
