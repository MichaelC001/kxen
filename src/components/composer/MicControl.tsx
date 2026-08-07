// 语音输入控制组：mic 按钮（录音=红 Mic+脉冲环）+ 引擎菜单 + 实发引擎/错误指示。
import { Show } from "solid-js";
import { Mic } from "lucide-solid";
import MicMenu from "./MicMenu";

export default function MicControl(props: {
  recording: () => boolean;
  activeVoice: () => string;
  voiceError: () => string;
  onToggle: () => void;
  onEngine: (e: string) => void;
}) {
  return (
    <>
      <div class="relative flex items-center">
        <button
          class="pressable action-icon mic-btn"
          classList={{ "mic-recording": props.recording() }}
          title={props.recording() ? "停止语音输入" : "语音输入（长按空格或点击）"}
          onClick={props.onToggle}
        >
          {/* 录音=live 非禁用：红 Mic+脉冲环；勿用 MicOff（斜杠=禁用语义） */}
          <Mic size={15} />
        </button>
        <MicMenu onEngine={props.onEngine} />
      </div>
      <Show when={props.recording() && props.activeVoice()}>
        <span class="text-2xs text-[var(--err)]">
          {props.activeVoice() === "apple"
            ? `${props.activeVoice()} 逐字`
            : `${props.activeVoice()} 整段转写`}
        </span>
      </Show>
      <Show when={props.voiceError()}>
        <span class="text-2xs text-[var(--err)]">{props.voiceError()}</span>
      </Show>
    </>
  );
}
