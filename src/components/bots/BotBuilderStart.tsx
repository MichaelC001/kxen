import { Show } from "solid-js";
import { actionClass, fieldClass, Panel, primaryClass } from "./shared";

interface BotBuilderStartProps {
  name: string;
  goal: string;
  builderId: string;
  acting: boolean;
  loadErr: string;
  setName: (value: string) => void;
  setGoal: (value: string) => void;
  setBuilderId: (value: string) => void;
  start: () => void;
  reload: () => void;
}

export default function BotBuilderStart(props: BotBuilderStartProps) {
  return (
    <div class="space-y-4">
      <Panel title="创建 Bot" detail="Bot Build Agent 只生成和验证定义，不继承最终 Bot 的权限。">
        <div class="space-y-2">
          <input
            class={fieldClass}
            value={props.name}
            onInput={(event) => props.setName(event.currentTarget.value)}
            placeholder="Bot 名称"
          />
          <textarea
            class={`${fieldClass} min-h-28`}
            value={props.goal}
            onInput={(event) => props.setGoal(event.currentTarget.value)}
            placeholder="要长期重复完成什么工作，输入、输出和成功标准是什么"
          />
          <button
            class={primaryClass}
            disabled={props.acting || !props.name.trim() || !props.goal.trim()}
            onClick={props.start}
          >
            生成 Bot 草稿
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
