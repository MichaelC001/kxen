// detectTrigger 纯函数：行首 \n 前界、全角边界、旧 \n 特判假阳性消除。
import { describe, expect, it } from "vitest";
import { detectTrigger } from "./triggers";

describe("detectTrigger", () => {
  it("行首触发：\\n 后的 @ / / # 全部生效", () => {
    expect(detectTrigger("第一行\n@sr", 7)).toEqual({ kind: "at", start: 4, query: "sr" });
    expect(detectTrigger("a\n/doc", 6)).toEqual({ kind: "slash", start: 2, query: "doc" });
    expect(detectTrigger("a\n#note", 7)).toEqual({ kind: "hash", start: 2, query: "note" });
    expect(detectTrigger("a\n/", 3)).toEqual({ kind: "slash", start: 2, query: "" });
  });

  it("光标紧贴 \n（触发符在光标处而非其前）不触发", () => {
    // 旧 \n 特判的假阳性：光标还没越过 / 就报了 slash
    expect(detectTrigger("\n/doc", 1)).toBeNull();
  });

  it("全角边界：全角空格与（【｛ 后可触发", () => {
    expect(detectTrigger("你好　@sr", 6)).toEqual({ kind: "at", start: 3, query: "sr" });
    expect(detectTrigger("（/doc", 5)).toEqual({ kind: "slash", start: 1, query: "doc" });
    expect(detectTrigger("【#note", 6)).toEqual({ kind: "hash", start: 1, query: "note" });
    expect(detectTrigger("｛@a", 3)).toEqual({ kind: "at", start: 1, query: "a" });
  });

  it("query 不跨全角空格", () => {
    expect(detectTrigger("@foo　bar", 8)).toBeNull();
  });

  it("原有边界与拒绝不变", () => {
    expect(detectTrigger("帮我 /doc", 7)).toEqual({ kind: "slash", start: 3, query: "doc" });
    expect(detectTrigger("(@x", 3)).toEqual({ kind: "at", start: 1, query: "x" });
    expect(detectTrigger("src/comp", 8)).toBeNull();
    expect(detectTrigger("a@b", 3)).toBeNull();
  });
});
