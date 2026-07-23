import { Show } from "solid-js";
import { ChevronRight } from "lucide-solid";
import { statusDot } from "../lib/variants";

/** 工具活动卡（Cursor/Cline 单卡形态）：头部行（状态点 + 名称 + 参数摘要 + 展开箭头），
 *  输出收在同一张卡的折叠体内——调用和结果是一个整体，不是两行孤立的文本。 */
export default function ToolCard(props: {
  name: string;
  call: string;
  result?: string | undefined;
}) {
  const failed = () => props.result?.startsWith("ERROR") || props.result === "interrupted";
  return (
    <details class="group rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] text-xs overflow-hidden">
      <summary class="flex items-center gap-2 px-3 py-1.5 cursor-pointer select-none list-none">
        <span
          class={statusDot({
            tone: props.result === undefined ? "warn" : failed() ? "err" : "ok",
            pulse: props.result === undefined,
          })}
        />
        <span class="font-mono text-[var(--accent-hover)]">{props.name}</span>
        <span class="text-[var(--text-dim)] truncate flex-1 font-mono">{props.call}</span>
        <ChevronRight
          size={12}
          class="text-[var(--text-faint)] group-open:rotate-90 transition-transform duration-150 shrink-0"
        />
      </summary>
      <Show when={props.result !== undefined}>
        <pre class="selectable px-3 py-2 border-t border-[var(--border)] bg-[var(--code-bg)] text-[var(--text-dim)] whitespace-pre-wrap break-all max-h-64 overflow-auto">
          {props.result}
        </pre>
      </Show>
    </details>
  );
}
