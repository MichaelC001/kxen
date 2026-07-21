// 灾难操作防护：命令静态分析 + 路径守卫。规则族定义见 docs/design/05。
// forbidden 决策不可被 prompt / AGENTS.md / 项目规则覆盖；approval 档交由
// opencode permission 系统处理，本包只做 forbidden 硬拦截与结构化返回。

export type Verdict = { verdict: "allow" } | { verdict: "deny"; ruleId: string; reason: string; suggestion?: string }

const HOME = process.env.HOME ?? "~"

// F1 系统路径（命中即 forbidden）。
// 注意 macOS：/tmp 是 /private/tmp 软链，临时区必须放行；
// /private 不能整目录拦，只拦系统区子路径。
const SYSTEM_PATHS = [
  "/",
  "/System",
  "/usr",
  "/bin",
  "/sbin",
  "/etc",
  "/var",
  "/Library",
  "/private/etc",
  "/private/var",
  "/private/bin",
  "/private/sbin",
  "/private/System",
  "/boot",
  "/proc",
  "/sys",
  "/dev",
]

// F2 用户目录一级保护
const HOME_TOP = ["Documents", "Desktop", "Downloads", "Library", "Pictures", "Movies"]
const HOME_DOT = [".ssh", ".gnupg", ".aws", ".config", ".kube", ".docker"]
const SHELL_RC = [".zshrc", ".bashrc", ".bash_profile", ".zprofile", ".profile"]

export function protectedPaths(): string[] {
  return [
    ...SYSTEM_PATHS,
    ...HOME_TOP.map((d) => `${HOME}/${d}`),
    ...HOME_DOT.map((d) => `${HOME}/${d}`),
    ...SHELL_RC.map((f) => `${HOME}/${f}`),
  ]
}

// --- 命令文本拆解 ---

/** 按管道与连接符拆段（不处理引号内语义，引号内容整体保留为一个 token 段） */
export function splitSegments(command: string): string[] {
  return command
    .split(/\|\||&&|;|\|/)
    .map((s) => s.trim())
    .filter(Boolean)
}

/** 提取一段命令的 token（粗粒度：空白切分，去引号） */
export function tokensOf(segment: string): string[] {
  return segment
    .split(/\s+/)
    .filter(Boolean)
    .map((t) => t.replace(/^["']|["']$/g, ""))
}

/** 检测未求值变量（$VAR / ${VAR}），危险命令里出现则不可静态判定 */
export function hasUnevaluatedVar(text: string): boolean {
  return /\$\{?[A-Za-z_][A-Za-z0-9_]*\}?/.test(text)
}

/** 提取 token 里的路径参数（跳过 flag 与其值） */
export function pathsOf(tokens: string[]): string[] {
  const out: string[] = []
  let skipNext = false
  for (const t of tokens.slice(1)) {
    if (skipNext) {
      skipNext = false
      continue
    }
    if (t.startsWith("--")) {
      if (!t.includes("=")) skipNext = true
      continue
    }
    if (t.startsWith("-") && t.length > 1) continue
    out.push(t)
  }
  return out
}

/** 规范化路径：~ 展开、resolve 相对路径、解 ..、去尾斜杠 */
export function normalizePath(p: string, cwd: string): string {
  let s = p
  if (s === "~" || s.startsWith("~/")) s = HOME + s.slice(1)
  if (!s.startsWith("/")) s = `${cwd}/${s}`
  const parts: string[] = []
  for (const seg of s.split("/")) {
    if (seg === "" || seg === ".") continue
    if (seg === "..") {
      parts.pop()
      continue
    }
    parts.push(seg)
  }
  return "/" + parts.join("/")
}

// 豁免前缀：系统区里的常规可写位置，优先于保护清单放行。
// macOS 用户临时区在 /private/var/folders；/dev/null 等是常规重定向目标。
const EXEMPT_PREFIXES = ["/private/var/folders", "/private/tmp", "/dev/null", "/dev/stdout", "/dev/stderr", "/dev/tty"]

/** 路径命中判定：返回命中的保护条目与规则族 */
export function classifyPath(p: string, cwd: string): { hit: string; family: "F1" | "F2" | "F3" } | undefined {
  const norm = normalizePath(p, cwd)

  for (const exempt of EXEMPT_PREFIXES) {
    if (norm === exempt || norm.startsWith(exempt + "/")) return undefined
  }

  // F3: .git 目录及其内容（正则精确到段，避免误伤 foo.git）
  if (/(^|\/)\.git(\/|$)/.test(norm)) return { hit: ".git", family: "F3" }

  // F1: 系统路径——自身、之下、祖先均拦（系统目录内任何文件都不该被动）
  for (const guard of SYSTEM_PATHS) {
    if (guard === "/" ? norm === "/" : norm === guard || norm.startsWith(guard + "/") || guard.startsWith(norm + "/")) {
      return { hit: guard, family: "F1" }
    }
  }

  // F2: $HOME 本身（精确）+ 一级目录本身；祖先命中（rm -rf ~ 覆盖全部）
  if (norm === HOME) return { hit: HOME, family: "F2" }
  // 凭证目录：自身与内容全拦
  for (const guard of HOME_DOT.filter((d) => d !== ".config").map((d) => `${HOME}/${d}`)) {
    if (norm === guard || norm.startsWith(guard + "/")) return { hit: guard, family: "F2" }
  }
  // 防毁灭目录与 .config、shell rc：仅拦整体，内容放行
  const homeChildren = [
    ...HOME_TOP.map((d) => `${HOME}/${d}`),
    `${HOME}/.config`,
    ...SHELL_RC.map((f) => `${HOME}/${f}`),
  ]
  for (const guard of homeChildren) {
    if (norm === guard) return { hit: guard, family: "F2" }
  }
  return undefined
}

/** 路径是否命中保护清单（供简单布尔判定） */
export function isProtected(p: string, cwd: string): string | undefined {
  return classifyPath(p, cwd)?.hit
}

const deny = (ruleId: string, reason: string, suggestion?: string): Verdict => ({
  verdict: "deny",
  ruleId,
  reason,
  suggestion,
})

// --- 各规则族 ---

const DELETE_CMDS = new Set(["rm", "rmdir", "trash", "unlink", "shred"])
const MOVE_CMDS = new Set(["mv", "move"])
const DISK_PATTERNS = [
  /^\/?dd\b.*\bof=\/dev\//,
  /\bmkfs(\.|\b)/,
  /\bdiskutil\s+erase/,
  /\bhdiutil\s+erase/,
  /\bfdisk\b/,
  /\bparted\b/,
]
const SYSTEM_CMDS = [/^\s*sudo\s+.*\b(shutdown|reboot|halt)\b/, /\b(nvram|csrutil)\b/]
const CRED_CMDS = [/\bsecurity\s+delete-/, /\bgpg\s+--delete-secret-key/]
const DESTROY_CMDS = [
  { re: /\bterraform\s+destroy\b/, id: "F4", why: "terraform destroy 销毁基础设施" },
  { re: /\bdropdb\b/, id: "F4", why: "dropdb 删除整个数据库" },
  {
    re: /\b(psql|mysql|mongosh?|mongo|redis-cli)\b.*\b(drop\s+database|dropDatabase|flushall)/i,
    id: "F4",
    why: "数据库毁灭操作",
  },
  { re: /\bkubectl\s+delete\s+(ns|namespace|--all)\b/, id: "F4", why: "kubectl 命名空间/全量删除" },
  { re: /\baws\s+s3\s+rb\s+.*--force\b/, id: "F4", why: "aws s3 rb --force 删除整个 bucket" },
  { re: /\bgcloud\s+projects\s+delete\b/, id: "F4", why: "gcloud 项目删除" },
  { re: /\bdocker\s+system\s+prune\b.*(--volumes|-a\b)/, id: "F4", why: "docker system prune 卷/全量清理" },
]

function evalDeleteSegment(seg: string, cwd: string): Verdict | undefined {
  const tokens = tokensOf(seg)
  // 跳过提权前缀，真实命令在其后
  const cmd = tokens[0] === "sudo" || tokens[0] === "doas" ? tokens[1] : tokens[0]
  if (!cmd) return undefined

  const isDelete =
    DELETE_CMDS.has(cmd) ||
    (/^find\b/.test(seg) && /\s-delete\b|\s-exec\s+(rm|trash)\b/.test(seg)) ||
    /^\s*rsync\b.*--delete/.test(seg)
  const isMove = MOVE_CMDS.has(cmd)
  if (!isDelete && !isMove) return undefined

  // find -delete 的目标路径是第一个位置参数
  const targets = cmd === "find" ? pathsOf(tokens).slice(0, 1) : pathsOf(tokens)
  // mv 的目标影响也要查：源与目的任一命中保护清单都拦
  if (targets.length === 0 && isDelete && /\s(-[a-zA-Z]*r|-[a-zA-Z]*f)/.test(seg)) {
    return deny("F5", "递归/强制删除缺少可静态确定的目标路径", "明确写出完整目标路径后再执行")
  }
  for (const t of targets) {
    if (hasUnevaluatedVar(t)) {
      return deny("F5", `删除/移动目标含未求值变量 ${t}，无法静态判定`, "先 echo 展开确认实际路径")
    }
    const hit = classifyPath(t, cwd)
    if (hit) {
      return deny(
        hit.family,
        `${cmd} 的目标 ${t} 命中保护路径 ${hit.hit}`,
        "工作区内的具体子路径操作不受限，请缩小范围",
      )
    }
  }
  return undefined
}

function evalGitSegment(seg: string, cwd: string): Verdict | undefined {
  if (!/^\s*git\s/.test(seg)) return undefined
  if (/\bgit\s+update-ref\s+-d\b/.test(seg)) {
    return deny("F3", "git update-ref -d 删除 refs", "删除单个分支用 git branch -d（approval 档）")
  }
  if (/\bgit\s+push\b.*(--delete|:\s*$)/.test(seg) && !/\S+\s*$/.test(seg.replace(/--delete/, ""))) {
    return deny("F3", "git push --delete 未限定具体 ref")
  }
  if (/\bgit\s+branch\s+-D\s+\*/.test(seg)) {
    return deny("F3", "git branch -D 批量删除分支")
  }
  return undefined
}

function evalSegment(seg: string, cwd: string): Verdict | undefined {
  if (DISK_PATTERNS.some((re) => re.test(seg))) {
    return deny("F1", "磁盘级操作（dd/mkfs/erase/fdisk/parted）")
  }
  if (SYSTEM_CMDS.some((re) => re.test(seg))) {
    return deny("F1", "系统属性或系统级进程操作")
  }
  if (CRED_CMDS.some((re) => re.test(seg))) {
    return deny("F2", "凭证存储销毁（Keychain / GPG 私钥）")
  }
  const d = DESTROY_CMDS.find((x) => x.re.test(seg))
  if (d) return deny(d.id, d.why)
  return evalDeleteSegment(seg, cwd) ?? evalGitSegment(seg, cwd)
}

/** 主入口：评估一条 shell 命令文本 */
export function evaluateShellCommand(command: string, cwd: string): Verdict {
  // 防绕过：bash -c / eval / xargs 嵌套的命令递归评估内层
  const nested =
    command.match(/(?:bash|zsh|sh|fish)\s+-c\s+["']([^"']+)["']/) ?? command.match(/\beval\s+["']([^"']+)["']/)
  if (nested?.[1]) {
    const inner = evaluateShellCommand(nested[1], cwd)
    if (inner.verdict === "deny") return inner
  }

  for (const seg of splitSegments(command)) {
    const v = evalSegment(seg, cwd)
    if (v) return v
  }
  return { verdict: "allow" }
}

/** 路径守卫（最终防线）：write/edit/delete 的实际路径在 resolve 后比对 */
export function guardPath(p: string, cwd: string): Verdict {
  const hit = classifyPath(p, cwd)
  if (!hit) return { verdict: "allow" }
  return deny(hit.family, `路径 ${p} 命中保护路径 ${hit.hit}`, "工作区内的具体子路径操作不受限，请缩小范围")
}
