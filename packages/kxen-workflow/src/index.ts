// 动态 workflow runtime：模型自主写脚本，runtime 执行编排。
// 计划（循环/分支/中间结果）在代码里，主上下文只收最终结果。
// 原语：agent / pipeline / constraints / phase；缓存恢复对齐 design/03。

export interface AgentCallOptions {
  role?: string
  label?: string
  schema?: Record<string, unknown>
}

export interface SubagentResult {
  text: string
  /** 结构化输出（schema 提供时） */
  object?: unknown
}

export type AgentExecutor = (prompt: string, opts: AgentCallOptions) => Promise<SubagentResult>

export type ConstraintsProvider = () => Record<string, unknown>

export interface WorkflowRunState {
  id: string
  status: "running" | "paused" | "completed" | "failed" | "stopped"
  phases: { name: string; agentCount: number; startedAt: number; endedAt?: number }[]
  agentCalls: number
  result?: unknown
  error?: string
}

export type WorkflowEvent =
  | { type: "start"; id: string; resumed: boolean }
  | { type: "agent_done"; id: string; index: number; label?: string }
  | { type: "agent_replayed"; id: string; index: number }
  | { type: "phase"; id: string; name: string }
  | { type: "paused"; id: string }
  | { type: "completed"; id: string; agentCalls: number }
  | { type: "failed"; id: string; error: string }

export type EventListener = (event: WorkflowEvent) => void

interface CallCacheEntry {
  index: number
  prompt: string
  result: SubagentResult
}

export interface WorkflowRuntimeOptions {
  executeAgent: AgentExecutor
  constraintsProvider?: ConstraintsProvider
  maxAgentCalls?: number
  onEvent?: EventListener
}

interface WorkflowApi {
  agent(prompt: string, opts?: AgentCallOptions): Promise<SubagentResult>
  pipeline<T, R>(items: T[], fn: (item: T, index: number) => Promise<R>, opts?: { concurrency?: number }): Promise<R[]>
  constraints(): Record<string, unknown>
  phase(name: string): void
  readonly args: unknown
}

const AsyncFunction = Object.getPrototypeOf(async () => {}).constructor as new (
  ...args: string[]
) => (...fnArgs: unknown[]) => Promise<unknown>

export class WorkflowRuntime {
  private runs = new Map<string, { state: WorkflowRunState; cache: CallCacheEntry[]; pauseRequested: boolean }>()
  private nextId = 1

  constructor(private opts: WorkflowRuntimeOptions) {}

  private emit(event: WorkflowEvent) {
    this.opts.onEvent?.(event)
  }

  async run(
    script: string,
    args?: unknown,
    resumeFrom?: { id: string; cache: CallCacheEntry[] },
  ): Promise<WorkflowRunState> {
    const id = resumeFrom?.id ?? `wf-${this.nextId++}`
    const cache: CallCacheEntry[] = resumeFrom?.cache ? [...resumeFrom.cache] : []
    const state: WorkflowRunState = { id, status: "running", phases: [], agentCalls: 0 }
    const runEntry = { state, cache, pauseRequested: false }
    this.runs.set(id, runEntry)
    this.emit({ type: "start", id, resumed: !!resumeFrom })

    let callIndex = 0
    const api = this.buildApi(id, runEntry, () => callIndex++)
    try {
      const fn = new AsyncFunction(
        "agent",
        "pipeline",
        "constraints",
        "phase",
        "args",
        `"use strict"; return (async () => { ${script} })()`,
      )
      const result = await fn(api.agent, api.pipeline, api.constraints, api.phase, args)
      state.result = result
      if (runEntry.pauseRequested) {
        state.status = "paused"
        this.emit({ type: "paused", id })
      } else {
        state.status = "completed"
        this.emit({ type: "completed", id, agentCalls: state.agentCalls })
      }
    } catch (err) {
      state.status = "failed"
      state.error = err instanceof Error ? err.message : String(err)
      this.emit({ type: "failed", id, error: state.error })
    }
    return state
  }

  pause(id: string): void {
    const run = this.runs.get(id)
    if (run) run.pauseRequested = true
  }

  snapshot(id: string): { state: WorkflowRunState; cache: CallCacheEntry[] } | undefined {
    const run = this.runs.get(id)
    return run ? { state: run.state, cache: [...run.cache] } : undefined
  }

  private buildApi(
    id: string,
    runEntry: { state: WorkflowRunState; cache: CallCacheEntry[]; pauseRequested: boolean },
    getCallIndex: () => number,
  ): WorkflowApi {
    const { state, cache } = runEntry
    const maxCalls = this.opts.maxAgentCalls ?? 200

    const agent = async (prompt: string, opts: AgentCallOptions = {}): Promise<SubagentResult> => {
      const index = getCallIndex()
      const cached = cache.find((c) => c.index === index && c.prompt === prompt)
      if (cached) {
        this.emit({ type: "agent_replayed", id, index })
        return cached.result
      }
      if (state.agentCalls >= maxCalls) {
        throw new Error(`workflow 超过 agent 调用上限 (${maxCalls})`)
      }
      state.agentCalls++
      const result = await this.opts.executeAgent(prompt, opts)
      cache.push({ index, prompt, result })
      this.emit({ type: "agent_done", id, index, label: opts.label })
      return result
    }

    return {
      args: undefined,
      agent,
      pipeline: async <T, R>(
        items: T[],
        fn: (item: T, index: number) => Promise<R>,
        opts: { concurrency?: number } = {},
      ): Promise<R[]> => {
        const concurrency = Math.max(1, opts.concurrency ?? 4)
        const results: R[] = new Array(items.length)
        let cursor = 0
        const workers = Array.from({ length: Math.min(concurrency, items.length) }, async () => {
          for (;;) {
            const i = cursor++
            if (i >= items.length) return
            results[i] = await fn(items[i] as T, i)
          }
        })
        await Promise.all(workers)
        return results
      },
      constraints: () => this.opts.constraintsProvider?.() ?? {},
      phase: (name: string) => {
        state.phases.push({ name, agentCount: state.agentCalls, startedAt: Date.now() })
        this.emit({ type: "phase", id, name })
      },
    }
  }
}
