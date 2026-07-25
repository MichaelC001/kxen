// 触发词防抖检出 + 弹层装配：200ms 合并连续输入，命中后算锚点开弹层（无命中/空结果即关）。
import type { CommandInfo } from "../../lib/chat";
import { buildItems, detectTrigger, type PopupState, type Trigger } from "./triggers";
import type { RowChip } from "./RowChips";

export function createTriggerCheck(opts: {
  ta: () => HTMLTextAreaElement | undefined;
  text: () => string;
  commands: () => CommandInfo[];
  removeTriggerText: (trigger: Trigger, from?: number) => void;
  pushChip: (chip: Omit<RowChip, "id">) => void;
  insertAtCaret: (insert: string) => void;
  setPopup: (p: (PopupState & Trigger) | null) => void;
  updatePopupPos: () => void;
}): { run: () => void; dispose: () => void } {
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  function run() {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(async () => {
      const ta = opts.ta();
      const cursor = ta?.selectionStart ?? opts.text().length;
      const trigger = detectTrigger(opts.text(), cursor);
      if (!trigger) {
        opts.setPopup(null);
        return;
      }
      const items = await buildItems(trigger, opts.commands(), {
        onChip: (kind, ref, label) => {
          opts.removeTriggerText(trigger);
          opts.pushChip({ kind, ref, label, title: ref });
          opts.setPopup(null);
          ta?.focus();
        },
        onPlainInsert: (insert, start) => {
          opts.removeTriggerText(trigger, start);
          opts.insertAtCaret(insert);
          opts.setPopup(null);
          ta?.focus();
        },
      });
      if (items.length === 0) {
        opts.setPopup(null);
        return;
      }
      opts.updatePopupPos();
      opts.setPopup({ ...trigger, items, selected: 0 });
    }, 200);
  }

  return {
    run,
    dispose: () => {
      if (debounceTimer) clearTimeout(debounceTimer);
    },
  };
}
