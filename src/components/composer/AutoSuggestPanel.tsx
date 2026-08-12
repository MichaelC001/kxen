import { createEffect, For } from "solid-js";
import type { AutoSuggestState } from "./auto-suggest";

const SOURCE_LABEL: Record<string, string> = {
  local: "Local",
  semantic: "Embedding",
  llm: "LLM",
};

export default function AutoSuggestPanel(props: {
  state: AutoSuggestState;
  onHover: (index: number) => void;
  onApply: (index: number) => void;
}) {
  let root: HTMLDivElement | undefined;
  createEffect(() => {
    root?.querySelectorAll("button")[props.state.selected]?.scrollIntoView({ block: "nearest" });
  });
  return (
    <div
      ref={(element) => (root = element)}
      role="listbox"
      aria-label="上下文主动推荐"
      class="absolute bottom-full left-0 right-0 mb-2 max-h-64 overflow-auto rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] shadow-lg z-20"
    >
      <For each={props.state.items}>
        {(item, index) => (
          <button
            role="option"
            aria-selected={index() === props.state.selected ? "true" : "false"}
            class="w-full px-3 py-2 text-left hover:bg-[var(--bg-overlay)]"
            classList={{ "bg-[var(--bg-overlay)]": index() === props.state.selected }}
            onMouseDown={(event) => event.preventDefault()}
            onMouseEnter={() => props.onHover(index())}
            onClick={() => props.onApply(index())}
          >
            <div class="flex items-center gap-2 text-xs">
              <span class="truncate">{item.kind === "file" ? item.path : item.label}</span>
              <span class="ml-auto shrink-0 rounded border border-[var(--border)] px-1 text-2xs text-[var(--text-faint)]">
                {SOURCE_LABEL[item.source] ?? item.source}
              </span>
            </div>
            <div class="mt-0.5 truncate text-2xs text-[var(--text-faint)]">{item.reason}</div>
          </button>
        )}
      </For>
    </div>
  );
}
