import { describe, expect, test } from "bun:test"
import { evaluateShellCommand, guardPath, normalizePath, splitSegments, hasUnevaluatedVar } from "./index"

const cwd = "/Users/test/project"

describe("splitSegments", () => {
  test("管道与连接符拆段", () => {
    expect(splitSegments("ls | grep foo && rm -rf / ; echo hi")).toEqual(["ls", "grep foo", "rm -rf /", "echo hi"])
  })
})

describe("hasUnevaluatedVar", () => {
  test("识别变量", () => {
    expect(hasUnevaluatedVar("rm -rf $DIR/")).toBe(true)
    expect(hasUnevaluatedVar("rm -rf ${DIR}/")).toBe(true)
    expect(hasUnevaluatedVar("rm -rf /tmp/x")).toBe(false)
  })
})

describe("normalizePath", () => {
  test("~ 展开与 .. 解析", () => {
    expect(normalizePath("~/Documents", cwd)).toBe(`${process.env.HOME}/Documents`)
    expect(normalizePath("../..", "/a/b/c")).toBe("/a")
    expect(normalizePath("/usr/../etc/", cwd)).toBe("/etc")
  })
})

describe("F1 系统毁灭", () => {
  test.each([
    "rm -rf /",
    "rm -rf /usr",
    "rm -rf /System/Library",
    "sudo rm -rf /etc",
    "dd if=/dev/zero of=/dev/disk0",
    "mkfs.ext4 /dev/sda1",
    "diskutil eraseDisk JHFS+ New disk0",
    "find / -name x -delete",
  ])("deny: %s", (cmd) => {
    expect(evaluateShellCommand(cmd, cwd).verdict).toBe("deny")
  })
  test("允许: 删除工作区文件", () => {
    expect(evaluateShellCommand("rm -rf ./dist", cwd).verdict).toBe("allow")
    expect(evaluateShellCommand("rm /tmp/foo.log", cwd).verdict).toBe("allow")
  })
})

describe("F2 用户目录毁灭", () => {
  test.each(["rm -rf ~", "rm -rf ~/Documents", "trash ~/Desktop", "rm -rf ~/.ssh", "security delete-generic-password -s x"])(
    "deny: %s",
    (cmd) => {
      expect(evaluateShellCommand(cmd, cwd).verdict).toBe("deny")
    },
  )
  test("允许: 用户目录普通子路径", () => {
    expect(evaluateShellCommand("rm ~/Documents/draft.txt", cwd).verdict).toBe("allow")
  })
})

describe("F3 git 仓库毁灭", () => {
  test.each(["rm -rf .git", "rm -rf ./.git", "mv .git /tmp/trash", "git update-ref -d refs/heads/main"])(
    "deny: %s",
    (cmd) => {
      expect(evaluateShellCommand(cmd, cwd).verdict).toBe("deny")
    },
  )
  test("允许: 正常 git 操作", () => {
    expect(evaluateShellCommand("git reset --hard HEAD", cwd).verdict).toBe("allow")
    expect(evaluateShellCommand("git branch -d feature-x", cwd).verdict).toBe("allow")
  })
})

describe("F4 数据与基础设施", () => {
  test.each([
    "terraform destroy",
    "dropdb production",
    "kubectl delete ns production",
    "aws s3 rb s3://bucket --force",
    "docker system prune --volumes",
  ])("deny: %s", (cmd) => {
    expect(evaluateShellCommand(cmd, cwd).verdict).toBe("deny")
  })
})

describe("F5 防绕过", () => {
  test("bash -c 嵌套递归评估", () => {
    expect(evaluateShellCommand(`bash -c "rm -rf /usr"`, cwd).verdict).toBe("deny")
  })
  test("未求值变量的递归删除", () => {
    expect(evaluateShellCommand("rm -rf $DIR/", cwd).verdict).toBe("deny")
  })
})

describe("guardPath", () => {
  test("保护路径命中", () => {
    expect(guardPath("~/.ssh/id_rsa", cwd).verdict).toBe("deny")
    expect(guardPath(".git/config", cwd).verdict).toBe("deny")
    expect(guardPath("/etc/hosts", cwd).verdict).toBe("deny")
  })
  test("工作区放行", () => {
    expect(guardPath("src/index.ts", cwd).verdict).toBe("allow")
  })
})
