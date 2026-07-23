// 审批卡：Ask 档挂起命令的用户决定入口（允许/拒绝，决定后只读展示）。
import { Show } from "solid-js";
import type { ApprovalItem } from "../lib/items";

export default function ApprovalCard(props: {
  item: ApprovalItem;
  onRespond: (id: string, allow: boolean) => void;
}) {
  return (
    <div class="rounded-lg border border-[var(--warn)]/50 bg-[var(--warn)]/5 px-3 py-2.5 text-xs space-y-2">
      <div class="text-[var(--warn)]">审批请求：{props.item.reason}</div>
      <div class="selectable font-mono text-[var(--text-dim)] break-all">{props.item.command}</div>
      <Show
        when={!props.item.resolved}
        fallback={
          <div class="text-2xs text-[var(--text-faint)]">
            {props.item.resolved === "allowed" ? "已允许" : "已拒绝"}
          </div>
        }
      >
        <div class="flex gap-2">
          <button
            class="pressable px-2.5 py-1 rounded text-2xs bg-[var(--accent)] text-[var(--accent-contrast)]"
            onClick={() => props.onRespond(props.item.approvalId, true)}
          >
            允许
          </button>
          <button
            class="pressable px-2.5 py-1 rounded text-2xs border border-[var(--border)] text-[var(--err)]"
            onClick={() => props.onRespond(props.item.approvalId, false)}
          >
            拒绝
          </button>
        </div>
      </Show>
    </div>
  );
}
