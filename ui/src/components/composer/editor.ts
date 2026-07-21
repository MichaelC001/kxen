// Lexical 编辑器装配：mention 插入/提取/触发 token 移除/语音节点追加。
import {
  $getNodeByKey,
  $getRoot,
  $getSelection,
  $isRangeSelection,
  $isTextNode,
  $isElementNode,
  $createTextNode,
  createEditor,
  type LexicalEditor,
  type NodeKey,
} from "lexical";
import {
  $createMentionNode,
  $isMentionNode,
  MentionNode,
  type MentionData,
  type MentionKind,
} from "./MentionNode";

export function setupEditor(root: HTMLElement): LexicalEditor {
  const editor = createEditor({
    namespace: "kxen-composer",
    nodes: [MentionNode],
    onError: (error) => console.error("[composer]", error),
  });
  root.contentEditable = "true";
  editor.setRootElement(root);
  return editor;
}

export interface ExtractResult {
  text: string;
  mentions: MentionData[];
}

/** 提取发送载荷：文本 + mention 数据（遍历文档树）。 */
export function extractPayload(editor: LexicalEditor): ExtractResult {
  return editor.read(() => {
    const mentions: MentionData[] = [];
    const collect = (node: unknown) => {
      if ($isMentionNode(node)) {
        mentions.push(node.getData());
        return;
      }
      const children = (node as { getChildren?: () => unknown[] }).getChildren?.() ?? [];
      children.forEach(collect);
    };
    collect($getRoot());
    return { text: $getRoot().getTextContent(), mentions };
  });
}

/** 清空编辑器（发送后）。 */
export function clearEditor(editor: LexicalEditor): void {
  editor.update(() => {
    $getRoot().clear();
  });
}

/** 删除触发 token（@query 等，位于 caret 所在 text node 内）并在原位插入 mention + 空格。 */
export function replaceTriggerWithMention(
  editor: LexicalEditor,
  triggerStartInNode: number,
  caretOffset: number,
  mention: MentionData,
): void {
  editor.update(() => {
    const selection = $getSelection();
    if (!$isRangeSelection(selection)) return;
    const anchor = selection.anchor;
    const node = anchor.getNode();
    if (!$isTextNode(node)) return;
    const text = node.getTextContent();
    const before = text.slice(0, triggerStartInNode);
    const after = text.slice(caretOffset);
    node.setTextContent(before + after);
    node.select(before.length, before.length);
    const sel = $getSelection();
    if ($isRangeSelection(sel)) {
      sel.insertNodes([$createMentionNode(mention), $createTextNode(" ")]);
    }
  });
}

/** 在文档尾部追加/更新语音 text node（interim 反复更新同一节点，不重复产生）。 */
export function upsertVoiceText(
  editor: LexicalEditor,
  nodeKey: NodeKey | null,
  text: string,
): NodeKey | null {
  let key: NodeKey | null = nodeKey;
  editor.update(() => {
    if (key) {
      const existing = $getNodeByKey(key);
      if ($isTextNode(existing)) {
        existing.setTextContent(text);
        existing.select(text.length, text.length);
        return;
      }
      key = null; // 节点被删了（用户清空过）：重建
    }
    const node = $createTextNode(text);
    const root = $getRoot();
    const first = root.getFirstChild();
    if (first && $isElementNode(first)) {
      first.append(node);
    } else {
      root.append(node);
    }
    node.select(text.length, text.length);
    key = node.getKey();
  });
  return key;
}

/** 文档是否为空（placeholder 展示判断）。 */
export function isEditorEmpty(editor: LexicalEditor): boolean {
  return editor.read(() => {
    const root = $getRoot();
    const text = root.getTextContent().trim();
    return text === "" && !hasMention(root);
  });
}

function hasMention(node: unknown): boolean {
  if ($isMentionNode(node)) return true;
  const children = (node as { getChildren?: () => unknown[] }).getChildren?.() ?? [];
  return children.some(hasMention);
}

export type { MentionKind };
