import { Show } from "solid-js";

/** 工具活动卡片：<details> 原生折叠（高频元素，不加动画——瞬时展开）。 */
export default function ToolCard(props: { name: string; call: string; result?: string }) {
  const failed = () => props.result?.startsWith("ERROR") || /\berror\b/i.test(props.result ?? "");
  return (
    <details class="group rounded-md border border-[var(--border)] bg-[var(--bg-raised)] text-xs">
      <summary class="flex items-center gap-2 px-2.5 py-1.5 cursor-pointer select-none list-none">
        <span
          class="inline-block w-1.5 h-1.5 rounded-full shrink-0"
          classList={{
            "bg-[var(--warn)] animate-pulse": props.result === undefined,
            "bg-[var(--ok)]": props.result !== undefined && !failed(),
            "bg-[var(--err)]": failed(),
          }}
        />
        <span class="font-mono text-[var(--accent-hover)]">{props.name}</span>
        <span class="text-[var(--text-dim)] truncate flex-1">{props.call}</span>
        <span class="text-[var(--text-faint)] group-open:rotate-90 transition-transform duration-150">
          &gt;
        </span>
      </summary>
      <Show when={props.result !== undefined}>
        <pre class="px-2.5 pb-2 pt-1 text-[var(--text-dim)] whitespace-pre-wrap break-all max-h-64 overflow-auto border-t border-[var(--border)]">
          {props.result}
        </pre>
      </Show>
    </details>
  );
}
