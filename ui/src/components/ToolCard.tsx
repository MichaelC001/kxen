import { Show } from "solid-js";
import { ChevronRight } from "lucide-solid";
import { statusDot } from "../lib/variants";

/** 工具活动卡片：<details> 原生折叠（高频元素，不加动画——瞬时展开）。 */
export default function ToolCard(props: {
  name: string;
  call: string;
  result?: string | undefined;
}) {
  const failed = () => props.result?.startsWith("ERROR") || /\berror\b/i.test(props.result ?? "");
  return (
    <details class="group rounded-md border border-[var(--border)] bg-[var(--bg-raised)] text-xs">
      <summary class="flex items-center gap-2 px-2.5 py-1.5 cursor-pointer select-none list-none">
        <span
          class={statusDot({
            tone: props.result === undefined ? "warn" : failed() ? "err" : "ok",
            pulse: props.result === undefined,
          })}
        />
        <span class="font-mono text-[var(--accent-hover)]">{props.name}</span>
        <span class="text-[var(--text-dim)] truncate flex-1">{props.call}</span>
        <ChevronRight
          size={12}
          class="text-[var(--text-faint)] group-open:rotate-90 transition-transform duration-150 shrink-0"
        />
      </summary>
      <Show when={props.result !== undefined}>
        <pre class="px-2.5 pb-2 pt-1 text-[var(--text-dim)] whitespace-pre-wrap break-all max-h-64 overflow-auto border-t border-[var(--border)]">
          {props.result}
        </pre>
      </Show>
    </details>
  );
}
