import { For, Show } from "solid-js";
import type { BotConversation, BotRoutine, BotSummary } from "../../lib/bots";
import { actionClass, fieldClass, Panel, primaryClass, shortId } from "./shared";

interface BotRoutineFormProps {
  editing: BotRoutine | null;
  bots: BotSummary[];
  conversations: BotConversation[];
  botId: string;
  name: string;
  cron: string;
  timezone: string;
  input: string;
  contextMode: "isolated" | "continue_conversation";
  conversationId: string;
  revisionMode: "follow_current" | "pinned";
  failureThreshold: number;
  acting: boolean;
  valid: boolean;
  setBotId: (value: string) => void;
  setName: (value: string) => void;
  setCron: (value: string) => void;
  setTimezone: (value: string) => void;
  setInput: (value: string) => void;
  setContextMode: (value: "isolated" | "continue_conversation") => void;
  setConversationId: (value: string) => void;
  setRevisionMode: (value: "follow_current" | "pinned") => void;
  setFailureThreshold: (value: number) => void;
  save: () => void;
  reset: () => void;
}

export default function BotRoutineForm(props: BotRoutineFormProps) {
  return (
    <Panel
      title={props.editing ? "编辑 Routine" : "创建 Routine"}
      detail="Routine 只保存触发策略。每次 occurrence 都解析并固定 revision，再创建持久化 BotRun。"
    >
      <div class="space-y-2">
        <select
          class={fieldClass}
          value={props.botId}
          onChange={(event) => props.setBotId(event.currentTarget.value)}
        >
          <option value="">选择 Active Bot</option>
          <For each={props.bots}>
            {(bot) => <option value={bot.bot_id}>{bot.display_name || bot.bot_id}</option>}
          </For>
        </select>
        <input
          class={fieldClass}
          value={props.name}
          onInput={(event) => props.setName(event.currentTarget.value)}
          placeholder="Routine 名称"
        />
        <input
          class={fieldClass}
          value={props.cron}
          onInput={(event) => props.setCron(event.currentTarget.value)}
          placeholder="Cron，例如 0 9 * * *"
        />
        <input
          class={fieldClass}
          value={props.timezone}
          onInput={(event) => props.setTimezone(event.currentTarget.value)}
          placeholder="IANA timezone，例如 Asia/Dubai"
        />
        <textarea
          class={`${fieldClass} min-h-20`}
          value={props.input}
          onInput={(event) => props.setInput(event.currentTarget.value)}
          placeholder="每次运行的输入"
        />
        <select
          class={fieldClass}
          value={props.contextMode}
          onChange={(event) =>
            props.setContextMode(event.currentTarget.value as "isolated" | "continue_conversation")
          }
        >
          <option value="isolated">isolated</option>
          <option value="continue_conversation">continue_conversation</option>
        </select>
        <Show when={props.contextMode === "continue_conversation"}>
          <select
            class={fieldClass}
            value={props.conversationId}
            onChange={(event) => props.setConversationId(event.currentTarget.value)}
          >
            <option value="">选择 Conversation</option>
            <For each={props.conversations}>
              {(conversation) => (
                <option value={conversation.conversation_id}>
                  {conversation.kind} {shortId(conversation.conversation_id)}
                </option>
              )}
            </For>
          </select>
        </Show>
        <select
          class={fieldClass}
          value={props.revisionMode}
          onChange={(event) =>
            props.setRevisionMode(event.currentTarget.value as "follow_current" | "pinned")
          }
        >
          <option value="follow_current">follow_current</option>
          <option value="pinned">pinned current revision</option>
        </select>
        <label class="text-xs text-[var(--text-dim)]">
          连续失败暂停阈值
          <input
            class={`${fieldClass} mt-1`}
            type="number"
            min="1"
            max="255"
            value={props.failureThreshold}
            onInput={(event) => props.setFailureThreshold(Number(event.currentTarget.value))}
          />
        </label>
        <div class="flex gap-2">
          <button class={primaryClass} disabled={props.acting || !props.valid} onClick={props.save}>
            {props.editing ? "保存" : "创建"}
          </button>
          <Show when={props.editing}>
            <button class={actionClass} onClick={props.reset}>
              取消
            </button>
          </Show>
        </div>
      </div>
    </Panel>
  );
}
