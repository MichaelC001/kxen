// 触发检测（@ / / / #，Zed 边界规则）+ caret 坐标计算（弹窗锚点）。
import type { EditorState } from "lexical";
import { $getSelection, $isRangeSelection, $isTextNode } from "lexical";

export interface Trigger {
  kind: "at" | "slash" | "hash";
  query: string;
  /** 触发字符在 caret 所在 text node 内的偏移（删除 token 用）。 */
  startInNode: number;
  caretOffset: number;
  /** 弹窗锚点（viewport 坐标）。 */
  rect: DOMRect | null;
}

/** 从当前 selection 解析触发 token；无触发返回 null。 */
export function detectTrigger(state: EditorState): Trigger | null {
  return state.read(() => {
    const selection = $getSelection();
    if (!$isRangeSelection(selection) || !selection.isCollapsed()) return null;
    const anchor = selection.anchor;
    const node = anchor.getNode();
    if (!$isTextNode(node)) return null;
    const text = node.getTextContent();
    const caret = anchor.offset;

    // 向前找触发字符（@ / # 任意位置；/ 只在 node 起始）
    for (let i = caret - 1; i >= 0; i--) {
      const c = text[i];
      if (c === " " || c === "\n") {
        // 空格：查询结束；但 /query 中允许空格？不允许（命令名单词）
        if (i === caret - 1) continue;
        return null;
      }
      if (c === "@" || c === "#" || c === "/") {
        const prev = i === 0 ? "" : text[i - 1];
        const bounded =
          i === 0 || prev === " " || prev === "\n" || prev === "(" || prev === "[" || prev === "{";
        if (!bounded) return null;
        if (c === "/" && i !== 0) return null;
        const kind = c === "@" ? "at" : c === "/" ? "slash" : "hash";
        return {
          kind,
          startInNode: i,
          caretOffset: caret,
          query: text.slice(i + 1, caret),
          rect: caretRect(),
        };
      }
    }
    return null;
  });
}

/** caret 的 viewport 坐标（原生 selection range rect）。 */
function caretRect(): DOMRect | null {
  const sel = window.getSelection();
  if (!sel || sel.rangeCount === 0) return null;
  const range = sel.getRangeAt(0).cloneRange();
  range.collapse(true);
  const rects = range.getClientRects();
  return rects.length > 0 ? rects.item(0) : null;
}
