// 图片附件压缩：FileReader 全量 base64 会把 Retina 截图（5-10MB）原样发给模型，
// 统一 canvas 缩到长边 1568 再编码（PNG/JPEG 保持原格式）。
export const IMAGE_LONG_EDGE = 1568;

/** 等比缩到长边上限内；已在限内原样返回（不触发重编码，jpeg 免于二次压缩失真）。 */
export function fitLongEdge(
  w: number,
  h: number,
  longEdge: number = IMAGE_LONG_EDGE,
): { w: number; h: number } {
  const m = Math.max(w, h);
  if (m <= longEdge || m <= 0) return { w, h };
  const k = longEdge / m;
  return { w: Math.max(1, Math.round(w * k)), h: Math.max(1, Math.round(h * k)) };
}

function readAsDataUrl(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(String(r.result));
    r.onerror = () => reject(new Error("文件读取失败"));
    r.readAsDataURL(file);
  });
}

function loadImage(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const el = new Image();
    el.onload = () => resolve(el);
    el.onerror = () => reject(new Error("图片解码失败"));
    el.src = url;
  });
}

/** File -> dataUrl：png/jpeg 超长边先 canvas 缩放重编码；其他格式（gif/webp/bmp）原样读（canvas 重编码会丢帧/失真）。 */
export async function fileToImageDataUrl(
  file: File,
  longEdge: number = IMAGE_LONG_EDGE,
): Promise<string> {
  if (file.type !== "image/png" && file.type !== "image/jpeg") return readAsDataUrl(file);
  try {
    const url = URL.createObjectURL(file);
    try {
      const img = await loadImage(url);
      const { w, h } = fitLongEdge(img.naturalWidth, img.naturalHeight, longEdge);
      if (w === img.naturalWidth && h === img.naturalHeight) return readAsDataUrl(file);
      const canvas = document.createElement("canvas");
      canvas.width = w;
      canvas.height = h;
      const ctx = canvas.getContext("2d");
      if (!ctx) return readAsDataUrl(file);
      ctx.drawImage(img, 0, 0, w, h);
      return canvas.toDataURL(file.type, 0.92);
    } finally {
      URL.revokeObjectURL(url);
    }
  } catch {
    // 解码/canvas 不可用（畸形文件）时退回原样读取，发送侧仍能拿到内容
    return readAsDataUrl(file);
  }
}
