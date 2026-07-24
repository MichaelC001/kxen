// 回退确认条：dirty 门禁（工作区有未进检查点改动）时征求用户放行，样式对齐 ApprovalCard。
export default function RewindConfirm(props: { onConfirm: () => void; onCancel: () => void }) {
  return (
    <div class="mb-2 rounded-lg border border-[var(--warn)]/50 bg-[var(--warn)]/5 px-3 py-2.5 text-xs space-y-2">
      <div class="text-[var(--warn)]">工作区有未进检查点的改动，回退会丢弃这些改动且不可恢复。</div>
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
