import { Show } from "solid-js";
import { actionClass, fieldClass, Panel, primaryClass, type BotBuilderTarget } from "./shared";

interface BotBuilderStartProps {
  name: string;
  goal: string;
  builderId: string;
  acting: boolean;
  loadErr: string;
  target?: BotBuilderTarget | undefined;
  targetLoading: boolean;
  retryingStart: boolean;
  setName: (value: string) => void;
  setGoal: (value: string) => void;
  setBuilderId: (value: string) => void;
  start: () => void;
  reload: () => void;
  clearTarget: () => void;
}

export default function BotBuilderStart(props: BotBuilderStartProps) {
  return (
    <div class="space-y-4">
      <Show when={props.target}>
        {(target) => (
          <div class="rounded-lg border border-[var(--accent)]/50 bg-[var(--bg-raised)] p-3 text-xs flex items-center gap-3">
            <span>
              正在设计 <strong>{target().display_name || target().bot_id}</strong>
            </span>
            <span class="font-mono text-2xs text-[var(--text-faint)]">{target().bot_id}</span>
            <button
              class={`${actionClass} ml-auto`}
              disabled={props.acting}
              onClick={props.clearTarget}
            >
              创建新 Bot
            </button>
          </div>
        )}
      </Show>
      <Panel
        title={props.target ? "开始新的调整对话" : "创建 Bot"}
        detail="每个 Builder Session 只绑定一个目标 Bot。Builder 通过对话生成和验证定义，不继承最终 Bot 的权限。"
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
                ? "正在查找 Builder Session"
                : props.target
                  ? "开始调整"
                  : "开始构建对话"}
          </button>
        </div>
      </Panel>
      <Panel title="继续现有 Build" detail="输入 Builder Session ID 可在重启或切页后恢复。">
        <div class="flex gap-2">
          <input
            class={fieldClass}
            value={props.builderId}
            onInput={(event) => props.setBuilderId(event.currentTarget.value)}
            placeholder="builder_..."
          />
          <button class={actionClass} disabled={!props.builderId.trim()} onClick={props.reload}>
            加载
          </button>
        </div>
        <Show when={props.loadErr}>
          <p class="text-xs text-[var(--err)] mt-2">{props.loadErr}</p>
        </Show>
      </Panel>
    </div>
  );
}
