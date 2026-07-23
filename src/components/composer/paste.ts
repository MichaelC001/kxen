// 粘贴处理（Codex 式大粘贴折叠 + CRLF 归一）。
// 折叠结构：textarea 里放纯文本占位 token（天然可选中/删除/撤销），全文存 store，发送时展开还原。
// 参照 openai/codex chat_composer.rs 的 pending_pastes 结构，阈值对齐 1000 字符，另加 20 行上限。

export const LARGE_PASTE_CHARS = 1000;
export const LARGE_PASTE_LINES = 20;

export function normalizePaste(text: string): string {
  return text.replace(/\r\n?/g, "\n");
}

export function isLargePaste(text: string): boolean {
  return text.length > LARGE_PASTE_CHARS || text.split("\n").length > LARGE_PASTE_LINES;
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
