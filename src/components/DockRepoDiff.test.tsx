// DockRepoDiff：仓库改动分段（git status 口径）——diff.status/diff.file RPC 的唯一消费方。
import { render } from "solid-js/web";
import { afterEach, describe, expect, it, vi } from "vitest";

const h = vi.hoisted(() => ({
  diffStatus: vi.fn(async () => [] as { path: string; status: string }[]),
  diffFile: vi.fn(async (_path: string) => ""),
}));

vi.mock("../lib/chat-ops", async (importOriginal) => {
  // 铺开真实模块只桩 diff 两函数：同文件还有 worktree/workspace 封装被 DockWorktree 等引用
  const orig = await importOriginal<typeof import("../lib/chat-ops")>();
  return { ...orig, diffStatus: h.diffStatus, diffFile: h.diffFile };
});

// Markdown 重依赖：桩成纯文本直出，断言 diff 内容透传即可
vi.mock("./Markdown", () => ({ default: (p: { text: string }) => <pre>{p.text}</pre> }));

import DockRepoDiff from "./DockRepoDiff";

const flush = () => new Promise((r) => setTimeout(r, 0));

afterEach(() => {
  document.body.innerHTML = "";
  h.diffStatus.mockReset().mockResolvedValue([]);
  h.diffFile.mockReset().mockResolvedValue("");
});

describe("DockRepoDiff 仓库改动分段", () => {
  it("渲染 git status 条目（状态中文 + 路径）", async () => {
    h.diffStatus.mockResolvedValue([
      { path: "src/a.ts", status: "M" },
      { path: "b.txt", status: "??" },
    ]);
    const dispose = render(() => <DockRepoDiff />, document.body);
    await flush();
    const text = document.body.textContent ?? "";
    expect(text).toContain("src/a.ts");
    expect(text).toContain("修改");
    expect(text).toContain("b.txt");
    expect(text).toContain("未跟踪");
    dispose();
  });

  it("空状态：工作区无未提交改动", async () => {
    const dispose = render(() => <DockRepoDiff />, document.body);
    await flush();
    expect(document.body.textContent).toContain("工作区无未提交改动");
    dispose();
  });

  it("点击条目展开 diff，再点收起", async () => {
    h.diffStatus.mockResolvedValue([{ path: "src/a.ts", status: "M" }]);
    h.diffFile.mockResolvedValue("@@ -1 +1 @@\n-old\n+new");
    const dispose = render(() => <DockRepoDiff />, document.body);
    await flush();
    const row = [...document.body.querySelectorAll("button")].find((b) =>
      b.textContent?.includes("src/a.ts"),
    );
    row?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();
    expect(h.diffFile).toHaveBeenCalledWith("src/a.ts");
    expect(document.body.textContent).toContain("+new");
    row?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    await flush();
    expect(document.body.textContent).not.toContain("+new");
    dispose();
  });
});
