import { Show } from "solid-js";
import { actionClass, fieldClass, Panel, primaryClass, type BotBuilderTarget } from "./shared";

interface BotBuilderStartProps {
  name: string;
  goal: string;
  acting: boolean;
  loadErr: string;
  target?: BotBuilderTarget | undefined;
  targetLoading: boolean;
  retryingStart: boolean;
  setName: (value: string) => void;
  setGoal: (value: string) => void;
  start: () => void;
  clearTarget: () => void;
}

export default function BotBuilderStart(props: BotBuilderStartProps) {
  return (
    <div class="space-y-4">
      <Show when={props.target}>
        {(target) => (
          <div class="rounded-lg border border-[var(--accent)]/50 bg-[var(--bg-raised)] p-3 text-xs flex items-center gap-3">
            <span>
              正在与 <strong>{target().display_name || target().bot_id}</strong> 交互完善自身定义
            </span>
            <span class="font-mono text-2xs text-[var(--text-faint)]">{target().bot_id}</span>
            <button
              class={`${actionClass} ml-auto`}
              disabled={props.acting}
              onClick={props.clearTarget}
            >
              创建另一个 Bot
            </button>
          </div>
        )}
      </Show>
      <Panel
        title={
          props.target
            ? `开始与 ${props.target.display_name || props.target.bot_id} 的新对话`
            : "交互创建 Bot"
        }
        detail="每个 Bot 都有独立的 self-builder capability，可以通过对话创建或持续完善自己的定义。Owner 始终独占授权和发布。"
      >
        <div class="space-y-2">
          <input
            class={fieldClass}
            value={props.name}
            disabled={Boolean(props.target) || props.retryingStart}
            onInput={(event) => props.setName(event.currentTarget.value)}
            placeholder="Bot 名称"
          />
          <textarea
            class={`${fieldClass} min-h-28`}
            value={props.goal}
            disabled={props.retryingStart}
            onInput={(event) => props.setGoal(event.currentTarget.value)}
            placeholder={
              props.target
                ? "说明想调整的职责、输入、输出、能力或约束"
                : "要长期重复完成什么工作，输入、输出和成功标准是什么"
            }
          />
          <button
            class={primaryClass}
            disabled={
              props.acting || props.targetLoading || !props.name.trim() || !props.goal.trim()
            }
            onClick={props.start}
          >
            {props.retryingStart
              ? "重试同一个构建对话"
              : props.targetLoading
                ? "正在恢复该 Bot 的构建对话"
                : props.target
                  ? "开始新一轮调整"
                  : "开始交互创建"}
          </button>
          <Show when={props.loadErr}>
            <p class="text-xs text-[var(--err)]">{props.loadErr}</p>
          </Show>
        </div>
      </Panel>
    </div>
  );
}
