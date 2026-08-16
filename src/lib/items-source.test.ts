// 注入类消息的来源助手：前缀剥离（折叠卡标题）与 context 引用一行描述（Chat/Trajectory 共用口径）。
import { describe, expect, it } from "vitest";
import { describeContextItems, firstLine, userSource, userSourceBody } from "./items";

describe("userSourceBody 前缀剥离", () => {
  it("剥离 [teammate x] / [task notification] 前缀，与 userSource 判定同口径", () => {
    expect(userSourceBody("[teammate builder] 已完成重构")).toBe("已完成重构");
    expect(userSourceBody("[task notification] agent a (execution) finished:\ndone")).toBe(
      "agent a (execution) finished:\ndone",
    );
  });

  it("无前缀或伪前缀原样返回（不会误吃普通口信）", () => {
    expect(userSourceBody("帮我看看 [teammate x]")).toBe("帮我看看 [teammate x]");
    expect(userSourceBody("[teammate] 缺名")).toBe("[teammate] 缺名");
    // 与 userSource 互斥保证：source 为空时 body 必为原文
    const plain = "普通消息";
    expect(userSource(plain)).toBeUndefined();
    expect(userSourceBody(plain)).toBe(plain);
  });
});

describe("describeContextItems 引用来源描述", () => {
  it("各类型一行描述，逗号并列；note 不含正文（正文在展开体）", () => {
    expect(
      describeContextItems([
        { type: "file", path: "src/a.ts" },
        { type: "dir", path: "src/lib" },
        { type: "web", url: "https://example.com" },
        { type: "docs", url: "https://docs.example.com" },
        { type: "note", text: "长注记不泄漏进标题" },
      ]),
    ).toBe(
      "file src/a.ts，dir src/lib，web https://example.com，docs https://docs.example.com，note",
    );
  });
});

describe("firstLine 单行摘要", () => {
  it("取首行，超长截断 120 字符", () => {
    expect(firstLine("第一行\n第二行")).toBe("第一行");
    expect(firstLine("x".repeat(130))).toHaveLength(121);
  });
});
