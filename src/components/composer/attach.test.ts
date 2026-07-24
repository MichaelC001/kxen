// 附件路径解析：子目录命中、同名 size 消歧、逃逸候选拒绝。
import { describe, expect, it } from "vitest";
import { isSafeRelPath, resolveAttachPath } from "./attach";

describe("isSafeRelPath", () => {
  it("放行普通相对路径", () => {
    expect(isSafeRelPath("a.txt")).toBe(true);
    expect(isSafeRelPath("src/lib/a.txt")).toBe(true);
  });

  it("拒绝逃逸、绝对路径与反斜杠", () => {
    expect(isSafeRelPath("../secret.txt")).toBe(false);
    expect(isSafeRelPath("a/../../secret.txt")).toBe(false);
    expect(isSafeRelPath("/etc/passwd")).toBe(false);
    expect(isSafeRelPath("C:\\Windows\\system.ini")).toBe(false);
    expect(isSafeRelPath("a\\b.txt")).toBe(false);
    expect(isSafeRelPath("")).toBe(false);
  });
});

describe("resolveAttachPath", () => {
  it("子目录文件解析出完整相对路径", () => {
    const rel = resolveAttachPath("a.txt", 3, [{ path: "src/lib/a.txt", size: 3 }]);
    expect(rel).toBe("src/lib/a.txt");
  });

  it("同名文件按 size 消歧", () => {
    const candidates = [
      { path: "README.md", size: 10 },
      { path: "docs/README.md", size: 20 },
    ];
    expect(resolveAttachPath("README.md", 20, candidates)).toBe("docs/README.md");
    expect(resolveAttachPath("README.md", 10, candidates)).toBe("README.md");
  });

  it("同名同 size 无法唯一确定时放弃解析（不猜错文件）", () => {
    const candidates = [
      { path: "x/data.json", size: 5 },
      { path: "y/data.json", size: 5 },
    ];
    expect(resolveAttachPath("data.json", 5, candidates)).toBeNull();
  });

  it("workspace 内无此文件返回 null", () => {
    expect(resolveAttachPath("report.pdf", 9, [{ path: "src/a.txt", size: 9 }])).toBeNull();
  });

  it("逃逸候选被守卫拒绝", () => {
    const candidates = [
      { path: "../secret.txt", size: 1 },
      { path: "/etc/passwd", size: 1 },
    ];
    expect(resolveAttachPath("secret.txt", 1, candidates)).toBeNull();
    expect(resolveAttachPath("passwd", 1, candidates)).toBeNull();
  });
});
