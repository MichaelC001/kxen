import path from "path"
import os from "os"
import { Global } from "@kxen/core/global"

// 订阅凭证导入：daemon 启动时从官方 CLI 的凭证存储同步到 kxen auth.json。
// 官方 CLI 会轮换 OAuth token，本地副本极易过期——每次以官方存储的新鲜副本为准
// （expires 更大者胜出；官方副本缺失时保留现有条目）。

type OauthEntry = { type: "oauth"; access: string; refresh: string; expires: number; accountId?: string }
type ApiEntry = { type: "api"; key: string }
type Entry = OauthEntry | ApiEntry

const authFile = () => path.join(Global.Path.data, "auth.json")

async function readAuth(): Promise<Record<string, Entry>> {
  const file = Bun.file(authFile())
  if (!(await file.exists())) return {}
  return file.json().catch(() => ({}))
}

async function writeAuth(data: Record<string, Entry>) {
  await Bun.write(authFile(), JSON.stringify(data, null, 2), { mode: 0o600 })
}

function fresherOauth(imported: OauthEntry, existing: Entry | undefined): boolean {
  if (!existing || existing.type !== "oauth") return true
  return (imported.expires ?? 0) > (existing.expires ?? 0)
}

// --- 各订阅源 ---

async function importClaude(): Promise<OauthEntry | undefined> {
  const credFile = Bun.file(path.join(os.homedir(), ".claude", ".credentials.json"))
  let raw: string | undefined
  if (await credFile.exists()) {
    raw = await credFile.text()
  } else if (process.platform === "darwin") {
    const proc = Bun.spawn(["security", "find-generic-password", "-s", "Claude Code-credentials", "-w"], {
      stdout: "pipe",
      stderr: "pipe",
    })
    const out = await new Response(proc.stdout).text()
    if ((await proc.exited) === 0 && out.trim()) raw = out.trim()
  }
  if (!raw) return undefined
  try {
    const parsed = JSON.parse(raw) as {
      claudeAiOauth?: { accessToken: string; refreshToken: string; expiresAt: number }
    }
    const oauth = parsed.claudeAiOauth
    if (!oauth?.accessToken) return undefined
    return { type: "oauth", access: oauth.accessToken, refresh: oauth.refreshToken, expires: oauth.expiresAt }
  } catch {
    return undefined
  }
}

async function importCodex(): Promise<OauthEntry | undefined> {
  const file = Bun.file(path.join(os.homedir(), ".codex", "auth.json"))
  if (!(await file.exists())) return undefined
  try {
    const parsed = (await file.json()) as {
      tokens?: { access_token: string; refresh_token: string; account_id?: string }
      last_refresh?: string
    }
    const t = parsed.tokens
    if (!t?.access_token) return undefined
    // codex 文件不带 expires；access token 是 JWT，exp 字段可读则用之
    let expires = 0
    try {
      const payload = JSON.parse(atob(t.access_token.split(".")[1])) as { exp?: number }
      expires = (payload.exp ?? 0) * 1000
    } catch {}
    return {
      type: "oauth",
      access: t.access_token,
      refresh: t.refresh_token,
      expires,
      ...(t.account_id && { accountId: t.account_id }),
    }
  } catch {
    return undefined
  }
}

async function importGrok(): Promise<OauthEntry | undefined> {
  const file = Bun.file(path.join(os.homedir(), ".grok", "auth.json"))
  if (!(await file.exists())) return undefined
  try {
    const parsed = (await file.json()) as Record<
      string,
      { key?: string; refresh_token?: string; expires_at?: string | number }
    >
    // issuer 键控 map，取 expires 最新的一条
    let best: { key: string; refresh: string; expires: number } | undefined
    for (const entry of Object.values(parsed)) {
      if (!entry.key) continue
      let expires = 0
      if (typeof entry.expires_at === "string") {
        expires = Date.parse(entry.expires_at) || 0
      } else if (typeof entry.expires_at === "number") {
        expires = entry.expires_at
      }
      if (!best || expires > best.expires) best = { key: entry.key, refresh: entry.refresh_token ?? "", expires }
    }
    if (!best) return undefined
    return { type: "oauth", access: best.key, refresh: best.refresh, expires: best.expires }
  } catch {
    return undefined
  }
}

async function importKimi(): Promise<ApiEntry | undefined> {
  const file = Bun.file(path.join(os.homedir(), ".kimi-code", "credentials", "kimi-code.json"))
  if (!(await file.exists())) return undefined
  try {
    const parsed = (await file.json()) as { access_token?: string }
    if (!parsed.access_token) return undefined
    // kimi-for-coding 是 Bearer 直连，access token 作 api key
    return { type: "api", key: parsed.access_token }
  } catch {
    return undefined
  }
}

export type ImportResult = { provider: string; action: "imported" | "fresh" | "missing" }

export async function importSubscriptions(): Promise<ImportResult[]> {
  const data = await readAuth()
  const results: ImportResult[] = []

  const steps: Array<[string, () => Promise<Entry | undefined>]> = [
    ["anthropic", importClaude],
    ["openai", importCodex],
    ["xai", importGrok],
    ["kimi-for-coding", importKimi],
  ]

  for (const [provider, read] of steps) {
    const imported = await read().catch(() => undefined)
    if (!imported) {
      results.push({ provider, action: data[provider] ? "fresh" : "missing" })
      continue
    }
    const existing = data[provider]
    const shouldWrite =
      imported.type === "oauth" ? fresherOauth(imported, existing) : JSON.stringify(imported) !== JSON.stringify(existing)
    if (shouldWrite) {
      data[provider] = imported
      results.push({ provider, action: "imported" })
    } else {
      results.push({ provider, action: "fresh" })
    }
  }

  await writeAuth(data)
  return results
}
