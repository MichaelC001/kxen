// Mention 节点：@文件/目录/链接/图片/知识 以 inline token 嵌在文本流（整块选中/整块删除）。
import { DecoratorNode, type NodeKey, type SerializedLexicalNode, type Spread } from "lexical";

export type MentionKind = "file" | "dir" | "web" | "docs" | "image" | "knowledge";

export interface MentionData {
  kind: MentionKind;
  ref: string;
  label: string;
  preview?: string;
}

type SerializedMention = Spread<MentionData, SerializedLexicalNode>;

const KIND_DOT: Record<MentionKind, string> = {
  file: "var(--accent-hover)",
  dir: "var(--warn)",
  web: "var(--ok)",
  docs: "var(--ok)",
  image: "var(--accent)",
  knowledge: "var(--err)",
};

const KIND_BADGE: Record<MentionKind, string> = {
  file: "文件",
  dir: "目录",
  web: "链接",
  docs: "文档",
  image: "图片",
  knowledge: "知识",
};

export class MentionNode extends DecoratorNode<null> {
  __kind: MentionKind;
  __ref: string;
  __label: string;
  __preview?: string;

  static getType(): string {
    return "mention";
  }

  static clone(node: MentionNode): MentionNode {
    return new MentionNode(node.__kind, node.__ref, node.__label, node.__preview, node.__key);
  }

  static importJSON(json: SerializedMention): MentionNode {
    return new MentionNode(json.kind, json.ref, json.label, json.preview);
  }

  exportJSON(): SerializedMention {
    return {
      ...super.exportJSON(),
      kind: this.__kind,
      ref: this.__ref,
      label: this.__label,
      preview: this.__preview,
    };
  }

  constructor(kind: MentionKind, ref: string, label: string, preview?: string, key?: NodeKey) {
    super(key);
    this.__kind = kind;
    this.__ref = ref;
    this.__label = label;
    this.__preview = preview;
  }

  createDOM(): HTMLElement {
    const el = document.createElement("span");
    el.className = "mention-token";
    el.dataset.kind = this.__kind;
    el.dataset.ref = this.__ref;

    if (this.__kind === "image" && this.__preview) {
      const img = document.createElement("img");
      img.src = this.__preview;
      img.className = "mention-img";
      el.appendChild(img);
    } else {
      const dot = document.createElement("span");
      dot.className = "mention-dot";
      dot.style.background = KIND_DOT[this.__kind];
      el.appendChild(dot);
    }

    const badge = document.createElement("span");
    badge.className = "mention-badge";
    badge.textContent = KIND_BADGE[this.__kind];
    el.appendChild(badge);

    const label = document.createElement("span");
    label.className = "mention-label";
    label.textContent = this.__label;
    el.appendChild(label);
    return el;
  }

  updateDOM(): boolean {
    return false;
  }

  isInline(): boolean {
    return true;
  }

  getData(): MentionData {
    return { kind: this.__kind, ref: this.__ref, label: this.__label, preview: this.__preview };
  }
}

export function $createMentionNode(data: MentionData): MentionNode {
  return new MentionNode(data.kind, data.ref, data.label, data.preview);
}

export function $isMentionNode(node: unknown): node is MentionNode {
  return node instanceof MentionNode;
}
