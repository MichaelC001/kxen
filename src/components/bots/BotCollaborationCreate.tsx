import { For } from "solid-js";
import type { BotSummary } from "../../lib/bots";
import { fieldClass, Panel, primaryClass } from "./shared";

export default function BotCollaborationCreate(props: {
  bots: BotSummary[];
  members: string[];
  moderator: string;
  left: string;
  right: string;
  acting: boolean;
  toggleMember: (botId: string) => void;
  setModerator: (botId: string) => void;
  setLeft: (botId: string) => void;
  setRight: (botId: string) => void;
  createGroup: () => void;
  openDirect: () => void;
}) {
  return (
    <div class="grid grid-cols-1 lg:grid-cols-2 gap-4">
      <Panel
        title="创建 Bot Group"
        detail="选择 2 到 6 个已发布 Bot。Moderator 负责无 mention 指令的首轮编排，每个 Bot 仍使用自己的权限快照。"
      >
        <div class="grid grid-cols-2 gap-2 mb-3">
          <For
            each={props.bots}
            fallback={
              <p class="text-xs text-[var(--text-faint)] col-span-2">至少需要两个 Active Bot。</p>
            }
          >
            {(bot) => (
              <label class="text-xs flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={props.members.includes(bot.bot_id)}
                  onChange={() => props.toggleMember(bot.bot_id)}
                />
                {bot.display_name || bot.bot_id}
              </label>
            )}
          </For>
        </div>
        <select
          class={`${fieldClass} mb-2`}
          value={props.moderator}
          onChange={(event) => props.setModerator(event.currentTarget.value)}
        >
          <option value="">选择 Moderator</option>
          <For each={props.members}>
            {(id) => (
              <option value={id}>
                {props.bots.find((bot) => bot.bot_id === id)?.display_name || id}
              </option>
            )}
          </For>
        </select>
        <button
          class={primaryClass}
          disabled={
            props.acting || props.members.length < 2 || props.members.length > 6 || !props.moderator
          }
          onClick={props.createGroup}
        >
          创建 Group
        </button>
      </Panel>
      <Panel
        title="打开 Bot Direct"
        detail="仅当两个 Bot 都显式 allow_direct 并互相加入 allowed_peers 时成立。Owner 不能伪装成 Bot 发送 Direct 消息。"
      >
        <div class="flex gap-2 mb-2">
          <BotSelect value={props.left} bots={props.bots} onChange={props.setLeft} label="Bot A" />
          <BotSelect
            value={props.right}
            bots={props.bots}
            onChange={props.setRight}
            label="Bot B"
          />
        </div>
        <button
          class={primaryClass}
          disabled={props.acting || !props.left || !props.right || props.left === props.right}
          onClick={props.openDirect}
        >
          打开 Direct
        </button>
      </Panel>
    </div>
  );
}

function BotSelect(props: {
  value: string;
  bots: BotSummary[];
  onChange: (value: string) => void;
  label: string;
}) {
  return (
    <select
      class={fieldClass}
      value={props.value}
      onChange={(event) => props.onChange(event.currentTarget.value)}
    >
      <option value="">{props.label}</option>
      <For each={props.bots}>
        {(bot) => <option value={bot.bot_id}>{bot.display_name || bot.bot_id}</option>}
      </For>
    </select>
  );
}
