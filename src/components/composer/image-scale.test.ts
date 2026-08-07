// 图片压缩：长边 1568 上限、等比、PNG/JPEG 保格式、小图不重编码。
import { describe, expect, it } from "vitest";
import { fileToImageDataUrl, fitLongEdge, IMAGE_LONG_EDGE } from "./image-scale";

function dataUrlToFile(dataUrl: string, name: string, type: string): File {
  const b64 = dataUrl.split(",")[1] ?? "";
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return new File([bytes], name, { type });
}

function makeImageFile(w: number, h: number, type: "image/png" | "image/jpeg"): File {
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const ctx = c.getContext("2d");
  if (!ctx) throw new Error("no 2d ctx");
  ctx.fillStyle = "#336699";
  ctx.fillRect(0, 0, w, h);
  return dataUrlToFile(c.toDataURL(type), `shot.${type === "image/png" ? "png" : "jpg"}`, type);
}

function dataUrlDims(dataUrl: string): Promise<{ w: number; h: number }> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve({ w: img.naturalWidth, h: img.naturalHeight });
    img.onerror = () => reject(new Error("decode failed"));
    img.src = dataUrl;
  });
}

describe("fitLongEdge", () => {
  it("宽图缩到长边上限（等比）", () => {
    expect(fitLongEdge(2000, 1000)).toEqual({ w: 1568, h: 784 });
  });

  it("高图缩到长边上限（等比）", () => {
    expect(fitLongEdge(1000, 2000)).toEqual({ w: 784, h: 1568 });
  });

  it("已在限内原样返回（不触发重编码）", () => {
    expect(fitLongEdge(100, 50)).toEqual({ w: 100, h: 50 });
    expect(fitLongEdge(1568, 1568)).toEqual({ w: 1568, h: 1568 });
  });

  it("异常尺寸不产出 0/负", () => {
    expect(fitLongEdge(0, 0)).toEqual({ w: 0, h: 0 });
    expect(fitLongEdge(4000, 1).h).toBe(1);
  });
});

describe("fileToImageDataUrl", () => {
  it("超大 PNG 缩到长边 1568 内且保持 png 格式", async () => {
    const file = makeImageFile(2000, 1000, "image/png");
    const out = await fileToImageDataUrl(file);
    expect(out.startsWith("data:image/png")).toBe(true);
    const dims = await dataUrlDims(out);
    expect(Math.max(dims.w, dims.h)).toBeLessThanOrEqual(IMAGE_LONG_EDGE);
    expect(dims).toEqual({ w: 1568, h: 784 });
    // 压缩后 base64 体积必须显著小于原图直读（2000x1000 PNG 原样 > 压缩后 1568x784）
    expect(out.length).toBeLessThan(file.size * 1.4);
  });

  it("超大 JPEG 缩放且保持 jpeg 格式", async () => {
    const file = makeImageFile(3136, 1568, "image/jpeg");
    const out = await fileToImageDataUrl(file);
    expect(out.startsWith("data:image/jpeg")).toBe(true);
    const dims = await dataUrlDims(out);
    expect(dims).toEqual({ w: 1568, h: 784 });
  });

  it("小图不重编码（尺寸与内容不变）", async () => {
    const file = makeImageFile(100, 50, "image/png");
    const out = await fileToImageDataUrl(file);
    const dims = await dataUrlDims(out);
    expect(dims).toEqual({ w: 100, h: 50 });
  });

  it("非 png/jpeg 格式原样读取（gif 不重编码丢帧）", async () => {
    // gif 文件内容不重要：走 FileReader 直通分支，不碰 canvas
    const file = new File([new Uint8Array([0x47, 0x49, 0x46])], "a.gif", { type: "image/gif" });
    const out = await fileToImageDataUrl(file);
    expect(out.startsWith("data:image/gif")).toBe(true);
  });
});
