// 附件路径解析：子目录命中、同名 size 消歧、逃逸候选拒绝、对话框路径分类与授权读取。
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  baseName,
  isImagePath,
  isSafeRelPath,
  resolveAttachPath,
  resolvePickedPath,
} from "./attach";

// resolvePickedPath 的 RPC 两层 mock：fs.allow_path 登记授权，fs.read_attachment 读图片
const rpcMock = vi.hoisted(() => ({
  allow: null as unknown,
  read: null as unknown,
  allowFail: false,
  readFail: false,
}));
vi.mock("../../lib/client", () => ({
  client: {
    rpc: (method: string) => {
      if (method === "fs.allow_path") {
        return rpcMock.allowFail
          ? Promise.reject(new Error("denied"))
          : Promise.resolve(rpcMock.allow);
      }
      if (method === "fs.read_attachment") {
        return rpcMock.readFail
          ? Promise.reject(new Error("denied"))
          : Promise.resolve(rpcMock.read);
      }
      return Promise.reject(new Error(`unexpected ${method}`));
    },
  },
}));

afterEach(() => {
  rpcMock.allow = null;
  rpcMock.read = null;
  rpcMock.allowFail = false;
  rpcMock.readFail = false;
});

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

describe("isImagePath", () => {
  it("按扩展名判定图片（大小写不敏感）", () => {
    for (const p of [
      "/tmp/a.png",
      "/tmp/a.JPG",
      "/tmp/a.jpeg",
      "/x/y.gif",
      "/x/y.webp",
      "/x/y.bmp",
    ]) {
      expect(isImagePath(p), p).toBe(true);
    }
    for (const p of ["/tmp/a.txt", "/tmp/png", "/tmp/a.png.bak"]) {
      expect(isImagePath(p), p).toBe(false);
    }
  });
});

describe("baseName", () => {
  it("取路径末段", () => {
    expect(baseName("/tmp/dir/a.txt")).toBe("a.txt");
    expect(baseName("/a.txt")).toBe("a.txt");
    expect(baseName("a.txt")).toBe("a.txt");
  });
});

describe("resolvePickedPath", () => {
  it("工作区内文件 chip 引用 rel", async () => {
    rpcMock.allow = { path: "/w/src/a.txt", rel: "src/a.txt" };
    const r = await resolvePickedPath("s1", "/w/src/a.txt");
    expect(r).toEqual({
      ok: true,
      chip: { kind: "file", ref: "src/a.txt", label: "a.txt", title: "src/a.txt" },
    });
  });

  it("工作区外文件 chip 引用绝对路径", async () => {
    rpcMock.allow = { path: "/etc/hosts", rel: null };
    const r = await resolvePickedPath("s1", "/etc/hosts");
    expect(r).toEqual({
      ok: true,
      chip: { kind: "file", ref: "/etc/hosts", label: "hosts", title: "/etc/hosts" },
    });
  });

  it("图片走 base64 内联，label 用 basename、title 用绝对路径", async () => {
    rpcMock.allow = { path: "/tmp/pic.png", rel: null };
    rpcMock.read = { kind: "base64", media_type: "image/png", data: "QUJD" };
    const r = await resolvePickedPath("s1", "/tmp/pic.png");
    expect(r).toEqual({
      ok: true,
      chip: {
        kind: "image",
        ref: "data:image/png;base64,QUJD",
        label: "pic.png",
        title: "/tmp/pic.png",
        image: { media_type: "image/png", data: "QUJD" },
      },
    });
  });

  it("授权失败带原因返回（不再静默 null）", async () => {
    rpcMock.allowFail = true;
    const r = await resolvePickedPath("s1", "/gone.txt");
    expect(r.ok).toBe(false);
    expect(!r.ok && r.reason).toContain("授权失败");
    expect(!r.ok && r.reason).toContain("denied"); // 后端原因必须透出
  });

  it("图片读取失败带原因返回（超 2MB cap / 权限等后端文案透出）", async () => {
    rpcMock.allow = { path: "/tmp/pic.png", rel: null };
    rpcMock.readFail = true;
    const r = await resolvePickedPath("s1", "/tmp/pic.png");
    expect(r.ok).toBe(false);
    expect(!r.ok && r.reason).toContain("读取失败");
    expect(!r.ok && r.reason).toContain("denied");
  });

  it("图片读出文本视为损坏，带原因返回", async () => {
    rpcMock.allow = { path: "/tmp/pic.png", rel: null };
    rpcMock.read = { kind: "text", text: "not an image" };
    const r = await resolvePickedPath("s1", "/tmp/pic.png");
    expect(r.ok).toBe(false);
    expect(!r.ok && r.reason).toContain("读取失败");
  });
});
