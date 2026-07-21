// 图片粘贴：clipboard image -> data URL -> image mention 插入 + base64 存储。
import type { LexicalEditor } from "lexical";
import { $getSelection, $isRangeSelection } from "lexical";
import { $createMentionNode } from "./MentionNode";

export interface ImagePart {
  media_type: string;
  data: string;
}

export function createPasteHandler(
  editor: () => LexicalEditor | null,
  imageStore: Map<string, ImagePart>,
) {
  return function onPaste(e: ClipboardEvent) {
    const items = e.clipboardData?.items ?? [];
    for (const item of items) {
      if (!item.type.startsWith("image/")) continue;
      e.preventDefault();
      const file = item.getAsFile();
      const ed = editor();
      if (!file || !ed) continue;
      const reader = new FileReader();
      reader.onload = () => {
        const dataUrl = String(reader.result);
        const base64 = dataUrl.split(",")[1] ?? "";
        imageStore.set(dataUrl, { media_type: file.type, data: base64 });
        ed.update(() => {
          const sel = $getSelection();
          if ($isRangeSelection(sel)) {
            sel.insertNodes([
              $createMentionNode({
                kind: "image",
                ref: dataUrl,
                label: `图片 ${file.type.split("/")[1] ?? ""}`,
                preview: dataUrl,
              }),
            ]);
          }
        });
      };
      reader.readAsDataURL(file);
    }
  };
}
