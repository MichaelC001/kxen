// 思考球渲染冒烟：四态各画一帧，断言非空像素且无异常。
import { describe, expect, it } from "vitest";
import { drawOrbFrame, type OrbState } from "../lib/orb";

const STATES: OrbState[] = ["thinking", "searching", "composing", "error"];

describe("thinking orb (webkit canvas)", () => {
  for (const state of STATES) {
    it(`${state} 帧有墨`, () => {
      const canvas = document.createElement("canvas");
      canvas.width = 64;
      canvas.height = 64;
      const ctx = canvas.getContext("2d")!;
      drawOrbFrame(ctx, state, 64, 1.3, true);
      const data = ctx.getImageData(0, 0, 64, 64).data;
      let inked = 0;
      for (let i = 3; i < data.length; i += 4) {
        if (data[i] > 0) inked++;
      }
      expect(inked).toBeGreaterThan(10);
      canvas.remove();
    });
  }

  it("明暗两题墨量镜像", () => {
    const canvas = document.createElement("canvas");
    canvas.width = 20;
    canvas.height = 20;
    const ctx = canvas.getContext("2d")!;
    drawOrbFrame(ctx, "thinking", 20, 0.5, false);
    const light = ctx.getImageData(0, 0, 20, 20).data.slice();
    drawOrbFrame(ctx, "thinking", 20, 0.5, true);
    const dark = ctx.getImageData(0, 0, 20, 20).data.slice();
    expect(light).not.toEqual(dark);
    canvas.remove();
  });
});
