// 粘贴处理（大粘贴折叠 + CRLF 归一）。
// 折叠结构：textarea 里放纯文本占位 token（天然可选中/删除/撤销），全文存 store，发送时展开还原。
// 折叠阈值 1000 字符，另加 20 行上限。

export const LARGE_PASTE_CHARS = 1000;
export const LARGE_PASTE_LINES = 20;

export function normalizePaste(text: string): string {
  return text.replace(/\r\n?/g, "\n");
}

export function isLargePaste(text: string): boolean {
  return text.length > LARGE_PASTE_CHARS || text.split("\n").length > LARGE_PASTE_LINES;
}

export interface PastePlan {
  files: FileList | undefined;
  /** 归一后的文本（无文本为 ""） */
  text: string;
  /** 要手动插入（preventDefault + insertAtCaret）；false = 小净文本走原生（保留原生 undo 粒度） */
  manual: boolean;
  /** 大粘贴：插入折叠占位 token 而非全文 */
  large: boolean;
}

/**
 * 粘贴事件分流。手动接管三种：大粘贴（折叠占位）、混合剪贴板（files 早退会把文本静默吞掉）、
 * 含 \r（小粘贴也要 CRLF 归一，原生粘贴不过 normalizePaste）。
 */
export function planPaste(e: ClipboardEvent): PastePlan {
  const cd = e.clipboardData;
  const files = cd && cd.files.length > 0 ? cd.files : undefined;
  const raw = cd?.getData("text/plain") ?? "";
  const text = normalizePaste(raw);
  const large = isLargePaste(text);
  const manual = text !== "" && (large || files !== undefined || raw.includes("\r"));
  return { files, text, manual, large };
}

export interface PasteStore {
  add: (full: string) => string;
  expand: (text: string) => string;
  clear: () => void;
  size: () => number;
}

export function createPasteStore(): PasteStore {
  const map = new Map<string, string>();
  let seq = 0;
  return {
    add: (full) => {
      const token = `[Pasted #${++seq}]`;
      map.set(token, full);
      return token;
    },
    expand: (text) => {
      // Map 迭代器是 live 的，删除当前项安全，无需先拷贝
      for (const [token, full] of map) {
        if (!text.includes(token)) {
          map.delete(token); // 占位被用户删掉 = 放弃这段粘贴
          continue;
        }
        text = text.replaceAll(token, full);
      }
      return text;
    },
    clear: () => map.clear(),
    size: () => map.size,
  };
}
