// DockWorktree 删除三态与确认条：进行中禁用 / 失败 flashErr 带原因 / 成功 flashOk；
// dirty 或删分支先出行内确认条；活跃行禁删；切换成功后才置勾标。
import { render } from "solid-js/web";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const WT1 = { name: "wt1", path: "/repo/.kxen/worktrees/wt1", branch: "kxen/wt1" };

const h = vi.hoisted(() => ({
  list: vi.fn(async () => [] as { name: string; path: string; branch: string }[]),
  remove: vi.fn(async (_name: string, _deleteBranch?: boolean) => {}),
  create: vi.fn(),
  status: vi.fn(async (_path: string) => [] as { path: string; status: string }[]),
  switch: vi.fn(async (_path: string) => {}),
  statusline: vi.fn(async (_id: string) => ({ workdir: "/repo" })),
}));

vi.mock("../lib/chat", async (importOriginal) => {
  // 铺开真实模块只桩 6 个相关 RPC（全量 mock 会断 state.ts 的传递绑定，同 Dock.test.tsx）
  const orig = await importOriginal<typeof import("../lib/chat")>();
  return {
    ...orig,
    worktreeList: h.list,
    worktreeRemove: h.remove,
    worktreeCreate: h.create,
    worktreeStatus: h.status,
    workspaceSwitch: h.switch,
    statusline: h.statusline,
  };
});

import DockWorktree from "./DockWorktree";
import { flash } from "../lib/flash";

const flush = () => new Promise((r) => setTimeout(r, 0));

function btn(title: string): HTMLButtonElement {
  const found = [...document.body.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) => b.title === title,
  );
  if (!found) throw new Error(`button not found: ${title}`);
  return found;
}

function btnByText(text: string): HTMLButtonElement {
  const found = [...document.body.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) => b.textContent === text,
  );
  if (!found) throw new Error(`button not found: ${text}`);
  return found;
}

beforeEach(() => {
  h.list.mockResolvedValue([WT1]);
  h.status.mockResolvedValue([]);
  h.statusline.mockResolvedValue({ workdir: "/repo" });
  h.switch.mockResolvedValue(undefined);
  h.remove.mockResolvedValue(undefined);
});

afterEach(() => {
  document.body.innerHTML = "";
  for (const m of flash.msgs()) flash.dismiss(m.id);
  vi.clearAllMocks();
});

describe("DockWorktree 删除三态", () => {
  it("进行中禁用连点，成功后 flashOk 并重拉列表", async () => {
    let release: () => void = () => {};
    h.remove.mockImplementation(() => new Promise<void>((r) => (release = r)));
    const dispose = render(() => <DockWorktree />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("kxen/wt1"));

    const removeBtn = btn("移除 worktree（分支保留）");
    expect(removeBtn.disabled).toBe(false);
    removeBtn.click();
    await flush();
    expect(h.remove).toHaveBeenCalledTimes(1);
    expect(btn("移除 worktree（分支保留）").disabled).toBe(true); // 进行中禁用
    btn("移除 worktree（分支保留）").click(); // 连点被拒
    await flush();
    expect(h.remove).toHaveBeenCalledTimes(1);

    h.list.mockResolvedValue([]); // 删后重拉为空
    release();
    await flush();
    await flush();
    expect(flash.msgs().some((m) => m.kind === "ok" && m.text.includes("已移除 worktree"))).toBe(
      true,
    );
    expect(flash.msgs().some((m) => m.kind === "err")).toBe(false);
    await vi.waitFor(() => expect(document.body.textContent).not.toContain("kxen/wt1"));
    dispose();
  });

  it("失败 flashErr 带后端原因，不假装成功", async () => {
    h.remove.mockRejectedValue(new Error("worktree is locked"));
    const dispose = render(() => <DockWorktree />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("kxen/wt1"));

    btn("移除 worktree（分支保留）").click();
    await flush();
    await flush();
    const err = flash.msgs().find((m) => m.kind === "err");
    expect(err?.text).toContain("删除失败");
    expect(err?.text).toContain("worktree is locked"); // 原因必须上屏
    expect(flash.msgs().some((m) => m.kind === "ok")).toBe(false);
    expect(document.body.textContent).toContain("kxen/wt1"); // 行还在
    dispose();
  });

  it("dirty 或删分支先出行内确认条，确认后才发 RPC", async () => {
    h.status.mockResolvedValue([{ path: "a.txt", status: "M" }]);
    const dispose = render(() => <DockWorktree />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("1 改"));

    btn("移除并删除分支").click();
    await flush();
    expect(h.remove).not.toHaveBeenCalled(); // 未确认不发 RPC
    expect(document.body.textContent).toContain("分支 kxen/wt1 将被删除（不可恢复）");
    expect(document.body.textContent).toContain("1 处未提交改动将丢失");

    btnByText("取消").click(); // 取消不收 RPC、不留条
    await flush();
    expect(h.remove).not.toHaveBeenCalled();
    expect(document.body.textContent).not.toContain("确认删除");

    btn("移除并删除分支").click();
    await flush();
    btnByText("确认删除").click();
    await flush();
    await flush();
    expect(h.remove).toHaveBeenCalledWith("wt1", true);
    expect(flash.msgs().some((m) => m.kind === "ok" && m.text.includes("已删除 kxen/wt1"))).toBe(
      true,
    );
    dispose();
  });

  it("活跃行删除按钮禁用并带说明", async () => {
    h.statusline.mockResolvedValue({ workdir: WT1.path }); // wt1 是活跃树
    const dispose = render(() => <DockWorktree />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("kxen/wt1"));

    const tip = "当前活跃 worktree 不可删除（先切换到其他目录）";
    const disabled = [...document.body.querySelectorAll<HTMLButtonElement>("button")].filter(
      (b) => b.title === tip,
    );
    expect(disabled.length).toBe(2); // 移除 + 删分支都禁
    for (const b of disabled) expect(b.disabled).toBe(true);
    dispose();
  });

  it("切换失败不置勾标并 flashErr，成功后才把行标为活跃", async () => {
    h.switch.mockRejectedValueOnce(new Error("permission denied"));
    const dispose = render(() => <DockWorktree />, document.body);
    await vi.waitFor(() => expect(document.body.textContent).toContain("kxen/wt1"));

    btn("切换工作区到此树（会话跑在该隔离目录）").click();
    await flush();
    await flush();
    const err = flash.msgs().find((m) => m.kind === "err");
    expect(err?.text).toContain("切换失败");
    expect(err?.text).toContain("permission denied");
    // 失败不置态：行仍未活跃（切换按钮还在）
    expect(document.body.textContent).toContain("切换");

    btn("切换工作区到此树（会话跑在该隔离目录）").click(); // 第二次成功
    await flush();
    await flush();
    expect(flash.msgs().some((m) => m.kind === "ok" && m.text.includes("已切换到 kxen/wt1"))).toBe(
      true,
    );
    // 置为活跃后：切换按钮消失，删除按钮禁用并换成说明 title
    expect(
      [...document.body.querySelectorAll("button")].some((b) => b.textContent === "切换"),
    ).toBe(false);
    const tip = "当前活跃 worktree 不可删除（先切换到其他目录）";
    const disabled = [...document.body.querySelectorAll<HTMLButtonElement>("button")].filter(
      (b) => b.title === tip,
    );
    expect(disabled.length).toBe(2);
    for (const b of disabled) expect(b.disabled).toBe(true);
    dispose();
  });
});
