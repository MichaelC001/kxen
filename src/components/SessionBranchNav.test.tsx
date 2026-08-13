import { render } from "solid-js/web";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { SessionMeta } from "../lib/chat";
import SessionBranchNav from "./SessionBranchNav";

const root: SessionMeta = {
  id: "root",
  title: "根会话",
  directory: "/repo",
  created_at: 1,
  updated_at: 1,
};
const branch: SessionMeta = {
  ...root,
  id: "branch",
  title: "编辑分支",
  parent_id: "root",
  branch_root_id: "root",
  fork_kind: "edit",
  fork_point: {
    message_id: "m1",
    message_index: 1,
    message_created_at: 1,
    position: "before",
  },
};

afterEach(() => {
  document.body.innerHTML = "";
});

describe("SessionBranchNav", () => {
  it("展示分支位置、共享文件边界，并可回父分支和切换分支", () => {
    const onSwitch = vi.fn();
    const dispose = render(
      () => (
        <SessionBranchNav
          current={() => branch}
          sessions={() => [root, branch]}
          onSwitch={onSwitch}
        />
      ),
      document.body,
    );
    const select = document.body.querySelector<HTMLSelectElement>(
      'select[aria-label="切换对话分支"]',
    );
    expect(select).not.toBeNull();
    expect(select?.title).toContain("父会话第 1 条");
    expect(select?.title).toContain("Workspace 文件状态共享");
    expect(document.body.textContent).toContain("2/2");

    document.body.querySelector<HTMLButtonElement>('button[title^="返回父分支"]')?.click();
    expect(onSwitch).toHaveBeenCalledWith("root");
    select!.value = "root";
    select!.dispatchEvent(new Event("change", { bubbles: true }));
    expect(onSwitch).toHaveBeenLastCalledWith("root");
    dispose();
  });

  it("父分支不存在时禁用返回入口且保留当前分支", () => {
    const dispose = render(
      () => (
        <SessionBranchNav current={() => branch} sessions={() => [branch]} onSwitch={() => {}} />
      ),
      document.body,
    );
    expect(
      document.body.querySelector<HTMLButtonElement>('button[title^="父分支已删除"]')?.disabled,
    ).toBe(true);
    expect(document.body.textContent).toContain("父分支已删除");
    dispose();
  });
});
