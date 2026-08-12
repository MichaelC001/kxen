import { For, Show } from "solid-js";
import type { BotConversation, BotSummary } from "../../lib/bots";
import {
  actionClass,
  actorLabel,
  fieldClass,
  Panel,
  partText,
  primaryClass,
  statusClass,
} from "./shared";

export default function BotConversationDetail(props: {
  conversation: BotConversation;
  bots: BotSummary[];
  acting: boolean;
  addBotId: string;
  instruction: string;
  mentions: string[];
  everyone: boolean;
  setAddBotId: (value: string) => void;
  setInstruction: (value: string) => void;
  setMentions: (value: string[]) => void;
  setEveryone: (value: boolean) => void;
  addMember: () => void;
  removeMember: (botId: string) => void;
  setModerator: (botId: string) => void;
  mutate: (kind: "pause" | "resume" | "archive" | "stop") => void;
  post: () => void;
  cancelTask: (taskId: string) => void;
}) {
  const activeMembers = () =>
    Object.values(props.conversation.members).filter((member) => member.active);
  const name = (botId: string) =>
    props.bots.find((bot) => bot.bot_id === botId)?.display_name || botId;
  return (
    <>
      <Panel
        title={props.conversation.kind === "bot_group" ? "Bot Group" : "Bot Direct"}
        detail={`${props.conversation.lifecycle}，version ${props.conversation.event_version}`}
      >
        <div class="space-y-2 mb-3">
          <For each={activeMembers()}>
            {(member) => (
              <div class="flex items-center gap-2 text-xs rounded border border-[var(--border)] px-2 py-1.5">
                <span>{name(member.bot_id)}</span>
                <Show when={props.conversation.moderator_bot_id === member.bot_id}>
                  <span class="text-2xs text-[var(--accent-hover)]">Moderator</span>
                </Show>
                <Show when={props.conversation.kind === "bot_group"}>
                  <button
                    class={`${actionClass} ml-auto`}
                    disabled={props.acting || props.conversation.moderator_bot_id === member.bot_id}
                    onClick={() => props.setModerator(member.bot_id)}
                  >
                    设为 Moderator
                  </button>
                  <button
                    class={actionClass}
                    disabled={
                      props.acting ||
                      props.conversation.moderator_bot_id === member.bot_id ||
                      activeMembers().length <= 2
                    }
                    onClick={() => props.removeMember(member.bot_id)}
                  >
                    移除
                  </button>
                </Show>
              </div>
            )}
          </For>
        </div>
        <Show when={props.conversation.kind === "bot_group"}>
          <div class="flex gap-2 mb-3">
            <select
              class={fieldClass}
              value={props.addBotId}
              onChange={(event) => props.setAddBotId(event.currentTarget.value)}
            >
              <option value="">添加 Active Bot</option>
              <For
                each={props.bots.filter((bot) => !props.conversation.members[bot.bot_id]?.active)}
              >
                {(bot) => <option value={bot.bot_id}>{bot.display_name || bot.bot_id}</option>}
              </For>
            </select>
            <button
              class={actionClass}
              disabled={props.acting || !props.addBotId}
              onClick={props.addMember}
            >
              添加
            </button>
          </div>
        </Show>
        <div class="flex flex-wrap gap-2">
          <Show when={props.conversation.lifecycle === "active"}>
            <button class={actionClass} onClick={() => props.mutate("pause")}>
              Pause
            </button>
          </Show>
          <Show when={props.conversation.lifecycle === "paused"}>
            <button class={actionClass} onClick={() => props.mutate("resume")}>
              Resume
            </button>
          </Show>
          <button
            class={actionClass}
            disabled={props.conversation.lifecycle === "archived"}
            onClick={() => props.mutate("archive")}
          >
            Archive
          </button>
          <Show
            when={
              props.conversation.kind === "bot_group" && props.conversation.lifecycle === "active"
            }
          >
            <button class={actionClass} onClick={() => props.mutate("stop")}>
              Stop Group
            </button>
          </Show>
        </div>
      </Panel>

      <Show
        when={props.conversation.kind === "bot_group" && props.conversation.lifecycle === "active"}
      >
        <Panel
          title="发送 Owner 指令"
          detail="默认只投递 Moderator。可定向 mention 多个 Bot，或显式 everyone 并行投递全部成员。"
        >
          <textarea
            class={`${fieldClass} min-h-20`}
            value={props.instruction}
            onInput={(event) => props.setInstruction(event.currentTarget.value)}
            placeholder="说明协作目标和期望产物"
          />
          <div class="flex flex-wrap gap-3 my-2">
            <label class="text-xs flex gap-1.5">
              <input
                type="checkbox"
                checked={props.everyone}
                onChange={(event) => {
                  props.setEveryone(event.currentTarget.checked);
                  if (event.currentTarget.checked) props.setMentions([]);
                }}
              />
              everyone
            </label>
            <For each={activeMembers()}>
              {(member) => (
                <label class="text-xs flex gap-1.5">
                  <input
                    type="checkbox"
                    disabled={props.everyone}
                    checked={props.mentions.includes(member.bot_id)}
                    onChange={(event) =>
                      props.setMentions(
                        event.currentTarget.checked
                          ? [...props.mentions, member.bot_id]
                          : props.mentions.filter((id) => id !== member.bot_id),
                      )
                    }
                  />
                  @{name(member.bot_id)}
                </label>
              )}
            </For>
          </div>
          <button
            class={primaryClass}
            disabled={props.acting || !props.instruction.trim()}
            onClick={props.post}
          >
            投递指令
          </button>
        </Panel>
      </Show>

      <Panel
        title="Timeline 与 Tasks"
        detail="消息、Delivery、Run 和 Task 都是持久化状态，断线后从此快照恢复。"
      >
        <div class="max-h-80 overflow-auto space-y-2 selectable">
          <For
            each={props.conversation.messages}
            fallback={<p class="text-xs text-[var(--text-faint)]">暂无消息。</p>}
          >
            {(message) => (
              <div class="rounded border border-[var(--border)] p-2 text-xs">
                <div class="flex gap-2 text-2xs">
                  <span class="text-[var(--accent-hover)]">{actorLabel(message.actor)}</span>
                  <span class="text-[var(--text-faint)]">{message.kind}</span>
                </div>
                <p class="whitespace-pre-wrap">{partText(message.parts)}</p>
              </div>
            )}
          </For>
        </div>
        <div class="mt-3 space-y-2">
          <For each={Object.values(props.conversation.tasks)}>
            {(task) => (
              <div class="rounded border border-[var(--border)] p-2 text-xs flex items-center gap-2">
                <span>{task.title}</span>
                <span class={statusClass(task.status)}>{task.status}</span>
                <span class="text-[var(--text-faint)]">Owner: {task.owner_bot_id}</span>
                <Show when={!terminal(task.status)}>
                  <button
                    class={`${actionClass} ml-auto`}
                    onClick={() => props.cancelTask(task.task_id)}
                  >
                    Cancel
                  </button>
                </Show>
              </div>
            )}
          </For>
        </div>
      </Panel>
    </>
  );
}

function terminal(status: string): boolean {
  return ["completed", "failed", "canceled", "rejected", "blocked"].includes(status);
}
