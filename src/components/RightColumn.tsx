import { createSignal, For, Show, onCleanup, onMount } from "solid-js";
import { ChevronRight } from "lucide-solid";
import { onTopic } from "../lib/chat";
import { agentsTranscript, type TranscriptEntry } from "../lib/team";
import { statusDot } from "../lib/variants";
import { KIND_BADGE, STATUS_TEXT, STATUS_TONE } from "../lib/agent-display";
import { activeAgentFocus, activeSessionId, agents, setActiveAgentFocus } from "../lib/state";
import Dock from "./Dock";

/** 右列：上 = 子代理概览卡（点击选中 TopAgentBar chip，转录在 PrimaryContent 展示）；下 = 会话上下文 Dock。 */
export default function RightColumn() {
  return (
    <div class="w-full h-full flex flex-col bg-[var(--bg-raised)]">
      {/* 子代理窗格区 */}
      <Show when={agents().length > 0}>
        <div class="shrink-0 border-b border-[var(--border)]" style={{ "max-height": "45%" }}>
          <div class="overflow-y-auto h-full">
            <For each={agents()}>{(a) => <AgentPane name={a.name} />}</For>
          </div>
        </div>
      </Show>

      {/* 会话上下文 */}
      <div class="flex-1 min-h-0">
        <Dock />
      </div>
    </div>
  );
}

/** 单个子代理概览卡：状态行 + 最近输出预览，点击选中 TopAgentBar chip。 */
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
      classList={{ "bg-[var(--bg-overlay)]/60": activeAgentFocus() === props.name }}
      onClick={() => setActiveAgentFocus(props.name)}
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
