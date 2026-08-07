// 分组名：worktree 目录显示「树名 (worktree)」；撞名上提 worktree 用「仓库/树名」（worktrees/<名> 仍会撞）。
import { describe, expect, it } from "vitest";
import { baseName, groupName, parentName, parseWorktreePath, promotedName } from "./group-name";

describe("path name fallbacks", () => {
  it("handles trailing separators, single segments, and empty paths", () => {
    expect(baseName("/x/app/")).toBe("app");
    expect(baseName("")).toBe("");
    expect(parentName("app")).toBe("app");
    expect(parentName("")).toBe("");
  });
});

describe("parseWorktreePath", () => {
  it("命中 <repo>/.kxen/worktrees/<name>", () => {
    expect(parseWorktreePath("/repo/.kxen/worktrees/exp")).toEqual({ repo: "repo", name: "exp" });
    expect(parseWorktreePath("/a/b/my-proj/.kxen/worktrees/fix-1")).toEqual({
      repo: "my-proj",
      name: "fix-1",
    });
  });

  it("非 kxen worktree 路径返回 null（缺 .kxen 段 / 段数不足）", () => {
    expect(parseWorktreePath("/a/b/worktrees/exp")).toBeNull();
    expect(parseWorktreePath("/repo/not-kxen/worktrees/exp")).toBeNull();
    expect(parseWorktreePath("/repo/.kxen/not-worktrees/exp")).toBeNull();
    expect(parseWorktreePath("/repo/.kxen/worktrees")).toBeNull();
    expect(parseWorktreePath("/repo")).toBeNull();
  });
});

describe("groupName", () => {
  it("普通目录取 basename", () => {
    expect(groupName("/x/app")).toBe("app");
  });

  it("worktree 目录显示「树名 (worktree)」而不是完整路径", () => {
    expect(groupName("/repo/.kxen/worktrees/exp")).toBe("exp (worktree)");
  });
});

describe("promotedName", () => {
  it("普通目录上提为 parent/name", () => {
    expect(promotedName("/x/app")).toBe("x/app");
  });

  it("worktree 上提为「仓库/树名 (worktree)」：两仓同树名可区分", () => {
    expect(promotedName("/repoA/.kxen/worktrees/exp")).toBe("repoA/exp (worktree)");
    expect(promotedName("/repoB/.kxen/worktrees/exp")).toBe("repoB/exp (worktree)");
  });
});
