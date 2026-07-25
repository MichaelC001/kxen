// 触发弹窗逻辑（textarea 版）：@ / / / # 检测（Zed 边界规则）+ 弹窗装配 + token 移除。
import { fsComplete, type CommandInfo, type CompleteEntry } from "../../lib/chat";

export interface PopupState {
  kind: "at" | "slash" | "hash";
  query: string;
  start: number;
  items: PopupItem[];
  selected: number;
}

export interface PopupItem {
  label: string;
  detail?: string | undefined;
  badge?: string | undefined;
  apply: () => void;
}

export interface Trigger {
  kind: "at" | "slash" | "hash";
  start: number;
  query: string;
}

/** 触发 token 检测：光标前最近的 @ / / / #，前界为行首/空白/半全角括号（Zed 边界规则）。 */
export function detectTrigger(value: string, cursor: number): Trigger | null {
  let i = cursor - 1;
  while (i >= 0) {
    const c = value[i];
    if (c === "\n") break;
    if (c === "@" || c === "#" || c === "/") {
      const prev = i === 0 ? "" : value[i - 1];
      // \n 必须算前界否则行首触发符全失效；全角空格/括号是中文输入的天然分隔
      const bounded =
        i === 0 ||
        prev === " " ||
        prev === "\t" ||
        prev === "\n" ||
        prev === "(" ||
        prev === "[" ||
        prev === "{" ||
        prev === "　" ||
        prev === "（" ||
        prev === "【" ||
        prev === "｛";
      if (!bounded) return null;
      const kind = c === "@" ? "at" : c === "/" ? "slash" : "hash";
      return { kind, start: i, query: value.slice(i + 1, cursor) };
    }
    // 全角空格同半角：query 不跨空白，否则会把空白后的整段都当 query
    if ((c === " " || c === "　") && i !== cursor - 1) break;
    i--;
  }
  return null;
}

const KNOWLEDGE_TARGETS = [
  { ref: ".agents/notes/", label: "写入项目笔记", detail: ".agents/notes/（入 git 共享，克制）" },
  { ref: "~/.agents/notes/", label: "写入个人笔记", detail: "~/.agents/notes/（跨项目，默认）" },
];

export interface PopupActions {
  onChip: (kind: "file" | "dir" | "knowledge", ref: string, label: string) => void;
  onPlainInsert: (text: string, triggerStart: number) => void;
  onCloseToken: (triggerStart: number) => void;
}

/** 按触发类型装配弹窗条目（200ms 防抖由调用方控制）。 */
export async function buildItems(
  trigger: Trigger,
  commands: CommandInfo[],
  actions: PopupActions,
): Promise<PopupItem[]> {
  if (trigger.kind === "at") {
    const hits = await fsComplete(trigger.query, 10).catch(() => [] as CompleteEntry[]);
    return hits.map((h) => ({
      label: h.path,
      badge: h.kind === "dir" ? "dir" : undefined,
      apply: () => {
        actions.onChip(
          h.kind === "dir" ? "dir" : "file",
          h.path,
          h.path.split("/").pop() ?? h.path,
        );
        actions.onCloseToken(trigger.start);
      },
    }));
  }
  if (trigger.kind === "slash") {
    const q = trigger.query.toLowerCase();
    return commands
      .filter((c) => c.name.toLowerCase().includes(q))
      .slice(0, 10)
      .map((c) => ({
        label: `/${c.name}${c.argument_hint ? ` ${c.argument_hint}` : ""}`,
        detail: c.description,
        badge: c.kind,
        apply: () => actions.onPlainInsert(`/${c.name} `, trigger.start),
      }));
  }
  const q = trigger.query.toLowerCase();
  return KNOWLEDGE_TARGETS.filter((k) => k.label.toLowerCase().includes(q)).map((k) => ({
    label: k.label,
    detail: k.detail,
    badge: "knowledge",
    apply: () => {
      actions.onChip("knowledge", k.ref, k.label);
      actions.onCloseToken(trigger.start);
    },
  }));
}
