// 回退确认条：dirty 门禁（工作区有未进检查点改动）时征求用户放行，样式对齐 ApprovalCard。
// 上下文（回到哪条消息 / 丢弃几个文件）读 rewindPendingInfo 通道：Session 页只传两个回调，不穿透组件树。
import { Show } from "solid-js";
import { rewindPendingInfo } from "../lib/rewind";

export default function RewindConfirm(props: { onConfirm: () => void; onCancel: () => void }) {
  const count = () => rewindPendingInfo()?.dirtyCount;
  const preview = () => rewindPendingInfo()?.targetPreview;
  const roleLabel = () =>
    rewindPendingInfo()?.targetRole === "assistant" ? "助手消息" : "我的消息";
  return (
    <div class="mb-2 rounded-lg border border-[var(--warn)]/50 bg-[var(--warn)]/5 px-3 py-2.5 text-xs space-y-2">
      <div class="text-[var(--warn)]">
        {count() == null
          ? "工作区有未进检查点的改动，回退会丢弃这些改动且不可恢复。"
          : `工作区有 ${count()} 个文件未进检查点，回退会丢弃这些改动且不可恢复。`}
      </div>
      <Show when={preview()}>
        <div class="text-[var(--text-dim)]">
          回退到{roleLabel()}：「{preview()}」
        </div>
      </Show>
      <div class="flex gap-2">
        <button
          class="pressable px-2.5 py-1 rounded text-2xs bg-[var(--accent)] text-[var(--accent-contrast)]"
          onClick={props.onConfirm}
        >
          丢弃改动并回退
        </button>
        <button
          class="pressable px-2.5 py-1 rounded text-2xs border border-[var(--border)] text-[var(--text-dim)]"
          onClick={props.onCancel}
        >
          取消
        </button>
      </div>
    </div>
  );
}
