import { For, Show } from "solid-js";
import type { BotConversation } from "../../lib/bots";
import { Panel, shortId, statusClass } from "./shared";

interface Props {
  conversations: BotConversation[];
  selectedId: string;
  loadError: string;
  onSelect: (conversationId: string) => void;
}

export default function BotConversationList(props: Props) {
  return (
    <Panel title="协作空间" detail="这里是 Bot collaboration timeline，不是多人聊天。">
      <Show when={props.loadError}>
        <p class="text-xs text-[var(--err)] mb-2">{props.loadError}</p>
      </Show>
      <div class="space-y-2">
        <For
          each={props.conversations}
          fallback={<p class="text-xs text-[var(--text-faint)]">暂无 Bot-to-Bot Conversation。</p>}
        >
          {(conversation) => (
            <button
              class="pressable w-full text-left rounded border border-[var(--border)] p-2"
              classList={{
                "border-[var(--accent)]": props.selectedId === conversation.conversation_id,
              }}
              onClick={() => props.onSelect(conversation.conversation_id)}
            >
              <div class="flex gap-2">
                <span class="text-xs">
                  {conversation.kind === "bot_group" ? "Group" : "Direct"}
                </span>
                <span class={`ml-auto text-2xs ${statusClass(conversation.lifecycle)}`}>
                  {conversation.lifecycle}
                </span>
              </div>
              <div class="font-mono text-2xs text-[var(--text-faint)]">
                {shortId(conversation.conversation_id)}
              </div>
            </button>
          )}
        </For>
      </div>
    </Panel>
  );
}
