// composer 附件装配（从 TextComposer 拆出，350 行门禁收口）：三种入口统一成 chip。
// 图片内联 base64；文件存路径引用（工作区外经 fs.allow_path 授权，见 attach.ts）。
import { ensureActiveSession } from "../../lib/state";
import { fsResolveName, resolveAttachPath, resolvePickedPath } from "./attach";
import type { RowChip } from "./RowChips";

export interface AttachDeps {
  images: Map<string, { media_type: string; data: string }>;
  pushChip: (chip: Omit<RowChip, "id">) => void;
}

export function createAttachments(deps: AttachDeps) {
  const { images, pushChip } = deps;

  /** 普通文件附件：File 只有 basename，反查 workspace 索引存相对路径（子目录可读、同名不串）。 */
  async function attachOneFile(file: File) {
    const candidates = await fsResolveName(file.name).catch(() => []);
    const rel = resolveAttachPath(file.name, file.size, candidates) ?? file.name;
    pushChip({ kind: "file", ref: rel, label: file.name, title: rel });
  }

  function attachFiles(files: FileList | File[]) {
    for (const file of files) {
      if (file.type.startsWith("image/")) {
        const reader = new FileReader();
        reader.onload = () => {
          const dataUrl = String(reader.result);
          images.set(dataUrl, { media_type: file.type, data: dataUrl.split(",")[1] ?? "" });
          pushChip({
            kind: "image",
            ref: dataUrl,
            label: `图片 ${file.type.split("/")[1] ?? ""}`,
            preview: dataUrl,
          });
        };
        reader.readAsDataURL(file);
      } else {
        void attachOneFile(file);
      }
    }
  }

  /** 原生对话框附件：真实绝对路径。授权绑会话（草稿态先落库）；图片读 base64 内联，文件走 context chip。 */
  async function attachPaths(paths: string[]) {
    const sid = await ensureActiveSession();
    for (const path of paths) {
      const chip = await resolvePickedPath(sid, path);
      if (!chip) continue;
      if (chip.kind === "image") {
        images.set(chip.ref, chip.image);
        pushChip({
          kind: "image",
          ref: chip.ref,
          label: chip.label,
          title: chip.title,
          preview: chip.ref,
        });
      } else {
        pushChip({ kind: "file", ref: chip.ref, label: chip.label, title: chip.title });
      }
    }
  }

  return { attachFiles, attachPaths };
}
