// Lexical composer 内核：editor 工厂 + 文本/chip 读写（token 模式整块删选）。
// Solid 集成纪律：root 元素零响应式绑定，editor 实例不落在任何 tracking scope 内重建。
import {
  $createParagraphNode,
  $createTextNode,
  $getRoot,
  $getSelection,
  $isRangeSelection,
  BEFORE_INPUT_COMMAND,
  COMMAND_PRIORITY_CRITICAL,
  createEditor,
  TextNode,
  type LexicalEditor,
} from "lexical";

export interface ChipData {
  kind: "file" | "dir" | "web" | "docs" | "image" | "knowledge";
  ref: string;
  label: string;
}

const TOKEN_STYLE =
  "background:var(--bg-overlay);border:1px solid var(--border);border-radius:4px;padding:0 4px;margin:0 1px;font-size:12px;";

export interface ComposerCore {
  editor: LexicalEditor;
  getText: () => string;
  setText: (t: string) => void;
  clear: () => void;
  insertChip: (chip: ChipData) => void;
  insertPlain: (text: string) => void;
  extractChips: () => ChipData[];
  focus: () => void;
  setCaret: (offset: number) => void;
  caretRect: () => DOMRect | null;
  caretOffset: () => number | null;
  onTextChange: (cb: () => void) => () => void;
}

/** 创建并挂载 editor。任何一步失败抛错（键入失效的历史教训：必须显式失败，不静默降级）。 */
export function mountComposer(el: HTMLElement): ComposerCore {
  const chipMap = new Map<string, ChipData>();
  const editor = createEditor({
    namespace: "kxen-composer",
    onError: (e: Error) => {
      console.error("[lexical]", e);
    },
  });
  // Lexical 0.48 core 的 setRootElement 不代管 contenteditable（webkit 实测 attr=null），wrapper 显式设置
  el.contentEditable = "true";
  el.setAttribute("role", "textbox");
  el.setAttribute("aria-multiline", "true");
  editor.setRootElement(el);
  // 显式探针：setRootElement 后元素必须可编辑（键入失效的历史教训：显式失败不静默降级）
  if (!el.isContentEditable) {
    throw new Error(
      `lexical setRootElement failed: not editable (attr=${el.getAttribute("contenteditable")}, prop=${el.contentEditable})`,
    );
  }

  // WebKit 实测：Lexical 的 insertText 快速通道（原生插入 + input 事件同步）在这环境断裂
  // （beforeinput 有、input 无）。受控模式接管纯文本插入：preventDefault + 手动 insertText。
  editor.registerCommand(
    BEFORE_INPUT_COMMAND,
    (event: InputEvent) => {
      if (event.inputType === "insertText" && event.data) {
        event.preventDefault();
        const data = event.data;
        editor.update(
          () => {
            const sel = $getSelection();
            if ($isRangeSelection(sel)) sel.insertText(data);
          },
          { discrete: true },
        );
        return true;
      }
      if (
        event.inputType === "deleteContentBackward" ||
        event.inputType === "deleteContentForward"
      ) {
        event.preventDefault();
        const backward = event.inputType === "deleteContentBackward";
        editor.update(
          () => {
            const sel = $getSelection();
            if ($isRangeSelection(sel)) sel.deleteCharacter(backward);
          },
          { discrete: true },
        );
        return true;
      }
      return false;
    },
    COMMAND_PRIORITY_CRITICAL,
  );

  const getText = () => editor.getEditorState().read(() => $getRoot().getTextContent());

  const setText = (t: string) => {
    editor.update(
      () => {
        const root = $getRoot();
        root.clear();
        const p = $createParagraphNode();
        if (t) p.append($createTextNode(t));
        root.append(p);
      },
      { discrete: true },
    );
  };

  const insertNode = (node: TextNode) => {
    const sel = $getSelection();
    if ($isRangeSelection(sel)) {
      sel.insertNodes([node]);
    } else {
      const root = $getRoot();
      let p = root.getLastChild();
      if (!p || p.getType() !== "paragraph") {
        p = $createParagraphNode();
        root.append(p);
      }
      (p as ReturnType<typeof $createParagraphNode>).append(node);
    }
  };

  return {
    editor,
    getText,
    setText,
    clear: () => {
      chipMap.clear();
      setText("");
    },
    insertChip: (chip) => {
      editor.update(
        () => {
          const node = $createTextNode(chip.label);
          node.setMode("token");
          node.setStyle(TOKEN_STYLE);
          chipMap.set(node.getKey(), chip);
          insertNode(node);
          insertNode($createTextNode(" "));
        },
        { discrete: true },
      );
    },
    insertPlain: (text) => {
      editor.update(() => insertNode($createTextNode(text)), { discrete: true });
    },
    extractChips: () => {
      const keys = new Set<string>();
      editor.getEditorState().read(() => {
        const walk = (n: unknown) => {
          const node = n as TextNode & { getChildren?: () => unknown[] };
          if (typeof node.getMode === "function" && node.getMode() === "token") {
            keys.add(node.getKey());
          }
          node.getChildren?.().forEach(walk);
        };
        walk($getRoot());
      });
      // 树里已不存在的 token（被删掉）自动出列
      const out: ChipData[] = [];
      for (const key of chipMap.keys()) {
        if (keys.has(key)) out.push(chipMap.get(key)!);
        else chipMap.delete(key);
      }
      return out;
    },
    focus: () => editor.focus(),
    setCaret: (offset: number) => {
      editor.update(
        () => {
          let remaining = offset;
          let placed = false;
          const walk = (node: unknown): void => {
            if (placed) return;
            if (node instanceof TextNode) {
              const len = node.getTextContentSize();
              if (remaining <= len) {
                node.select(remaining, remaining);
                placed = true;
              } else {
                remaining -= len;
              }
              return;
            }
            const n = node as { getChildren?: () => unknown[]; getType?: () => string };
            n.getChildren?.().forEach(walk);
            if (!placed && n.getType?.() === "paragraph") remaining -= 1;
          };
          walk($getRoot());
          if (!placed) $getRoot().selectEnd();
        },
        { discrete: true },
      );
    },
    caretRect: () => {
      const sel = window.getSelection();
      if (!sel || sel.rangeCount === 0) return null;
      const range = sel.getRangeAt(0);
      const rect = range.getBoundingClientRect();
      return rect.width === 0 && rect.height === 0 ? null : rect;
    },
    caretOffset: () =>
      editor.getEditorState().read(() => {
        const sel = $getSelection();
        if (!$isRangeSelection(sel)) return null;
        const anchor = sel.anchor;
        let offset = 0;
        let found = false;
        const walk = (node: unknown): void => {
          if (found) return;
          const n = node as {
            getKey: () => string;
            getTextContentSize?: () => number;
            getChildren?: () => unknown[];
          };
          if (n.getKey() === anchor.key) {
            offset += anchor.offset;
            found = true;
            return;
          }
          const children = n.getChildren?.();
          if (children) {
            for (const c of children) walk(c);
            // 段落边界算一个换行（与 getTextContent 对齐）
            if (!found && (n as { getType?: () => string }).getType?.() === "paragraph")
              offset += 1;
          }
        };
        walk($getRoot());
        return found ? offset : null;
      }),
    onTextChange: (cb) =>
      editor.registerUpdateListener(({ dirtyElements, dirtyLeaves }) => {
        if (dirtyElements.size > 0 || dirtyLeaves.size > 0) cb();
      }),
  };
}
