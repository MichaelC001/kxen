// 全局模型资源管理（MRM）：角色路由 + 并发/限额 + 降级链 + 状态可见。
// 作用在 provider 层，与具体功能无关——任何模型调用与 subagent 派发都经此。

export interface RoleBinding {
  provider: string
  model: string
}

export interface ProviderLimit {
  concurrent?: number
  rpm?: number
}

export interface MrmConfig {
  roles: Record<string, RoleBinding>
  /** role -> 降级角色链（目标满员时按序尝试） */
  fallbacks: Record<string, string[]>
  globalConcurrent: number
  providers: Record<string, ProviderLimit>
}

export const DEFAULT_CONFIG: MrmConfig = {
  roles: {},
  fallbacks: {},
  globalConcurrent: 8,
  providers: {},
}

export interface ResolveResult {
  provider: string
  model: string
  /** 发生降级时为原角色名 */
  degradedFrom?: string
}

export interface ProviderStatus {
  inFlight: number
  limit: number
  queued: number
}

export interface MrmStatus {
  global: ProviderStatus
  providers: Record<string, ProviderStatus>
}

/** TOML 配置解析（[roles] / [limits] / [[roles.fallback]] 三段） */
export function parseConfig(toml: string): MrmConfig {
  const doc = (Bun.TOML.parse(toml) ?? {}) as Record<string, unknown>
  const out: MrmConfig = structuredClone(DEFAULT_CONFIG)

  const roles = doc.roles as Record<string, unknown> | undefined
  if (roles) {
    for (const [name, value] of Object.entries(roles)) {
      if (name === "fallback") continue
      const v = value as RoleBinding
      if (v?.provider && v?.model) out.roles[name] = { provider: v.provider, model: v.model }
    }
    const fb = roles.fallback as Array<{ role?: string; chain?: string[] }> | undefined
    if (Array.isArray(fb)) {
      for (const item of fb) {
        if (item?.role && Array.isArray(item.chain))
          out.fallbacks[item.role] = item.chain.filter((r) => r !== item.role)
      }
    }
  }

  const limits = doc.limits as Record<string, unknown> | undefined
  if (limits) {
    if (typeof limits.global_concurrent === "number") out.globalConcurrent = limits.global_concurrent
    const providers = limits.providers as Record<string, ProviderLimit> | undefined
    if (providers) {
      for (const [name, v] of Object.entries(providers)) {
        out.providers[name] = {
          concurrent: typeof v?.concurrent === "number" ? v.concurrent : undefined,
          rpm: typeof v?.rpm === "number" ? v.rpm : undefined,
        }
      }
    }
  }
  return out
}

export class ModelResourceManager {
  private inFlight = new Map<string, number>()
  private queued = new Map<string, Array<() => void>>()
  private windows = new Map<string, number[]>()

  constructor(private config: MrmConfig) {}

  /** 角色解析（不含降级；降级由调用方按 resolveRoleChain 结果决定） */
  role(role: string): RoleBinding | undefined {
    return this.config.roles[role]
  }

  /** 角色降级链：[原角色, ...fallbacks]，过滤掉未定义角色 */
  roleChain(role: string): string[] {
    return [role, ...(this.config.fallbacks[role] ?? [])].filter((r) => this.config.roles[r])
  }

  limitOf(provider: string): number {
    return this.config.providers[provider]?.concurrent ?? this.config.globalConcurrent
  }

  inFlightOf(provider: string): number {
    return this.inFlight.get(provider) ?? 0
  }

  /** provider 当前是否有空槽（含全局并发检查） */
  available(provider: string): boolean {
    if (this.inFlightOf("") >= this.config.globalConcurrent) return false
    return this.inFlightOf(provider) < this.limitOf(provider)
  }

  /** 角色 -> 可执行的 provider/model；全部满员时返回 undefined（调用方排队） */
  resolve(role: string): ResolveResult | undefined {
    let first: string | undefined
    for (const r of this.roleChain(role)) {
      const binding = this.config.roles[r]
      if (!binding) continue
      first ??= r
      if (this.available(binding.provider)) {
        return { provider: binding.provider, model: binding.model, degradedFrom: r === role ? undefined : role }
      }
    }
    return undefined
  }

  /** 占槽；返回释放函数。调用前需确保 available（否则返回 undefined） */
  tryAcquire(provider: string): (() => void) | undefined {
    if (!this.available(provider)) return undefined
    return this.acquireUnsafe(provider)
  }

  /** 占槽，满员则排队等待 */
  async acquire(provider: string): Promise<() => void> {
    const immediate = this.tryAcquire(provider)
    if (immediate) return immediate
    await new Promise<void>((resolve) => {
      const q = this.queued.get(provider) ?? []
      q.push(resolve)
      this.queued.set(provider, q)
    })
    return this.acquireUnsafe(provider)
  }

  private acquireUnsafe(provider: string): () => void {
    this.inFlight.set(provider, this.inFlightOf(provider) + 1)
    this.inFlight.set("", this.inFlightOf("") + 1)
    let released = false
    return () => {
      if (released) return
      released = true
      this.inFlight.set(provider, Math.max(0, this.inFlightOf(provider) - 1))
      this.inFlight.set("", Math.max(0, this.inFlightOf("") - 1))
      const q = this.queued.get(provider)
      const next = q?.shift()
      if (next) next()
    }
  }

  /** RPM 窗口检查（1 分钟滑动窗）；超限时返回需等待的毫秒数 */
  rpmWaitMs(provider: string): number {
    const rpm = this.config.providers[provider]?.rpm
    if (!rpm) return 0
    const now = Date.now()
    const window = (this.windows.get(provider) ?? []).filter((t) => now - t < 60_000)
    this.windows.set(provider, window)
    if (window.length < rpm) {
      window.push(now)
      return 0
    }
    return 60_000 - (now - window[0])
  }

  status(): MrmStatus {
    const providers: Record<string, ProviderStatus> = {}
    for (const name of new Set([...Object.keys(this.config.providers), ...this.inFlight.keys()])) {
      if (name === "") continue
      providers[name] = {
        inFlight: this.inFlightOf(name),
        limit: this.limitOf(name),
        queued: this.queued.get(name)?.length ?? 0,
      }
    }
    return {
      global: {
        inFlight: this.inFlightOf(""),
        limit: this.config.globalConcurrent,
        queued: [...this.queued.values()].reduce((a, q) => a + q.length, 0),
      },
      providers,
    }
  }

  /** 给规划模型的可读状态摘要 */
  describe(): string {
    const s = this.status()
    const lines = [`global: ${s.global.inFlight}/${s.global.limit} concurrent, ${s.global.queued} queued`]
    for (const [name, p] of Object.entries(s.providers)) {
      lines.push(`${name}: ${p.inFlight}/${p.limit} concurrent, ${p.queued} queued`)
    }
    return lines.join("\n")
  }
}

/** 从 kxen 配置文件加载 MRM 配置（用户级 + 项目级合并，项目级覆盖） */
export async function loadConfig(paths: { user: string; project?: string }): Promise<MrmConfig> {
  let config = structuredClone(DEFAULT_CONFIG)
  for (const p of [paths.user, paths.project]) {
    if (!p) continue
    const file = Bun.file(p)
    if (!(await file.exists())) continue
    const parsed = parseConfig(await file.text())
    config = {
      roles: { ...config.roles, ...parsed.roles },
      fallbacks: { ...config.fallbacks, ...parsed.fallbacks },
      globalConcurrent:
        parsed.globalConcurrent !== DEFAULT_CONFIG.globalConcurrent ? parsed.globalConcurrent : config.globalConcurrent,
      providers: { ...config.providers, ...parsed.providers },
    }
  }
  return config
}
