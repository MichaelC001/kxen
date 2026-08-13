import { For } from "solid-js";
import type { BuilderState } from "../../lib/bots";
import { fieldClass, Panel, primaryClass } from "./shared";

export default function BotBuilderConversation(props: {
  builder: BuilderState;
  botName: string;
  message: string;
  acting: boolean;
  retrying: boolean;
  setMessage: (value: string) => void;
  send: () => void;
}) {
  return (
    <Panel
      title={`${props.botName} 的构建对话`}
      detail={`这是 ${props.botName} 自己的 durable self-builder 对话。它可以追问或更新自身草稿，但不能替 Owner 授权和发布。`}
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
                  {owner ? "Owner" : props.botName}
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
              ? `上一条 Owner 消息已固定，重试将继续 ${props.botName} 的同一个构建 turn`
              : `回复 ${props.botName}，或继续调整它的定义`
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
          {props.retrying ? `重试 ${props.botName} 回复` : "发送"}
        </button>
      </div>
    </Panel>
  );
}
