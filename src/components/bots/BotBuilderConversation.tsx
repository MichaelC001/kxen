import { For } from "solid-js";
import type { BuilderState } from "../../lib/bots";
import { fieldClass, Panel, primaryClass } from "./shared";

export default function BotBuilderConversation(props: {
  builder: BuilderState;
  message: string;
  acting: boolean;
  retrying: boolean;
  setMessage: (value: string) => void;
  send: () => void;
}) {
  return (
    <Panel
      title="Builder 对话"
      detail="这个 durable 对话只设计当前目标 Bot。Builder 可以追问或更新草稿，但不能替 Owner 授权和发布。"
    >
      <div class="max-h-72 overflow-auto space-y-2 mb-3 selectable" aria-live="polite">
        <For
          each={props.builder.messages}
          fallback={
            <p class="text-xs text-[var(--text-faint)]">
              说明这个 Bot 的职责、输入、输出、成功标准和需要的能力。
            </p>
          }
        >
          {(item) => {
            const owner = item.actor.kind === "owner";
            return (
              <div
                class="rounded border px-3 py-2 text-xs"
                classList={{
                  "ml-8 border-[var(--accent)]/50 bg-[var(--bg-overlay)]": owner,
                  "mr-8 border-[var(--border)]": !owner,
                }}
              >
                <div class="text-2xs text-[var(--text-faint)] mb-1">
                  {owner ? "Owner" : "Bot Builder"}
                </div>
                <p class="whitespace-pre-wrap">{item.text}</p>
              </div>
            );
          }}
        </For>
      </div>
      <div class="flex gap-2">
        <textarea
          class={`${fieldClass} min-h-20`}
          value={props.message}
          disabled={props.retrying}
          onInput={(event) => props.setMessage(event.currentTarget.value)}
          placeholder={
            props.retrying
              ? "上一条 Owner 消息已固定，重试将继续同一个 Builder turn"
              : "回答 Builder，或继续调整这个 Bot"
          }
        />
        <button
          class={primaryClass}
          disabled={
            props.acting ||
            (!props.retrying && !props.message.trim()) ||
            props.builder.lifecycle !== "active"
          }
          onClick={props.send}
        >
          {props.retrying ? "重试 Builder 回复" : "发送"}
        </button>
      </div>
    </Panel>
  );
}
