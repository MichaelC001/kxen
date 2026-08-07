// 排队预览：run 进行中已入队的消息数 + 首条预览 + 清空。
import { Show } from "solid-js";

export default function PendingQueue(props: { queue: () => string[]; onClear: () => void }) {
  return (
    <Show when={props.queue().length > 0}>
      <div class="flex items-center gap-2 px-2 pb-1.5 text-2xs text-[var(--text-dim)]">
        <span class="inline-block w-1 h-1 rounded-full bg-[var(--warn)] animate-pulse shrink-0" />
        <span class="truncate flex-1">
          排队中 {props.queue().length} 条 · {props.queue()[0]}
        </span>
        <button
          class="pressable px-1.5 py-0.5 rounded border border-[var(--border)]"
          onClick={props.onClear}
        >
          清空
        </button>
      </div>
    </Show>
  );
}
