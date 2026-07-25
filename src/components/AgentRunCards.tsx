// 时间线内嵌的 agent 状态卡区：状态点 + 名称 + model 小字 + 状态文案一行卡。
// 点击卡切主区看该 run 的转录；running 卡出停止钮、终态卡出关闭钮（动作逻辑在 agent-run.tsx）。
// agents() 由 refreshAgents(activeSessionId) 拉取，天然只含当前会话，无需再按 session 过滤。
import { For, Show } from "solid-js";
import { activeAgentFocus, agents, setActiveAgentFocus } from "../lib/state";
import { kindBadge, statusText, statusTone } from "../lib/agent-display";
import { statusDot } from "../lib/variants";
import { AgentRunActionButtons, useAgentRunActions } from "./agent-run";

export default function AgentRunCards() {
  const { stopping, stopAgent, dismissAgent } = useAgentRunActions();

  return (
    <Show when={agents().length > 0}>
      <div class="space-y-1.5" data-agent-run-cards>
        <For each={agents()}>
          {(a) => (
            <div class="group relative">
              <button
                data-run-card
                class="pressable w-full flex items-center gap-2 px-3 py-2 rounded border border-[var(--border)]/50 text-xs text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/40"
                classList={{
                  "bg-[var(--bg-overlay)]/60 text-[var(--text)]": activeAgentFocus() === a.name,
                  "opacity-50": stopping() === a.name,
                }}
                disabled={stopping() === a.name}
                title={`${kindBadge(a.kind)} · ${statusText(a.status)} · ${a.model.model}`}
                onClick={() => setActiveAgentFocus(a.name)}
              >
                <span class={statusDot(statusTone(a.status))} />
                <span class="font-medium max-w-32 truncate">{a.name}</span>
                <span class="text-2xs px-1 rounded border border-[var(--border)] text-[var(--text-faint)]">
                  {kindBadge(a.kind)}
                </span>
                <span class="text-2xs text-[var(--text-faint)] max-w-24 truncate">
                  {a.model.model}
                </span>
                <span class="text-2xs text-[var(--text-faint)] ml-auto shrink-0">
                  {statusText(a.status)}
                </span>
              </button>
              <AgentRunActionButtons
                name={a.name}
                status={a.status}
                stopping={stopping() === a.name}
                class="right-2 top-1/2 -translate-y-1/2"
                onStop={(n) => void stopAgent(n)}
                onDismiss={(n) => void dismissAgent(n)}
              />
            </div>
          )}
        </For>
      </div>
    </Show>
  );
}
