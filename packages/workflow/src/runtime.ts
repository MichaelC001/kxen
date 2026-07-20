import type { EventBus } from '@kxen/core';
import type { SubagentResult } from '@kxen/subagent';
import type {
	AgentCallOptions,
	AgentExecutor,
	ConstraintsProvider,
	WorkflowApi,
	WorkflowRunState,
} from './api';

export interface WorkflowRuntimeOptions {
	bus: EventBus;
	executeAgent: AgentExecutor;
	constraintsProvider?: ConstraintsProvider;
	// 规模护栏：单次 run 的 agent 调用上限（对齐 Claude 的 1000 硬顶，默认收紧）
	maxAgentCalls?: number;
}

interface CallCacheEntry {
	index: number;
	prompt: string;
	result: SubagentResult;
}

// workflow runtime：脚本化编排 + 后台执行 + agent 级缓存恢复（design/03、research/04）
export class WorkflowRuntime {
	private runs = new Map<
		string,
		{
			state: WorkflowRunState;
			cache: CallCacheEntry[];
			pauseRequested: boolean;
		}
	>();
	private nextId = 1;

	constructor(private opts: WorkflowRuntimeOptions) {}

	// 执行脚本；resumeFrom 提供上次 run 的缓存即可会话内恢复（已完成调用直接回放）
	async run(
		script: string,
		args?: unknown,
		resumeFrom?: { id: string; cache: CallCacheEntry[] },
	): Promise<WorkflowRunState> {
		const id = resumeFrom?.id ?? `wf-${this.nextId++}`;
		const cache: CallCacheEntry[] = resumeFrom?.cache
			? [...resumeFrom.cache]
			: [];
		const state: WorkflowRunState = {
			id,
			status: 'running',
			phases: [],
			agentCalls: 0,
		};
		const runEntry = { state, cache, pauseRequested: false };
		this.runs.set(id, runEntry);
		this.opts.bus.publish('workflow.start', { id, resumed: !!resumeFrom });

		let callIndex = 0;
		const api = this.buildApi(id, runEntry, () => callIndex++);
		try {
			const fn = new AsyncFunction(
				'agent',
				'pipeline',
				'constraints',
				'phase',
				'args',
				`"use strict"; return (async () => { ${script} })()`,
			);
			const result = await fn(
				api.agent,
				api.pipeline,
				api.constraints,
				api.phase,
				args,
			);
			state.result = result;
			if (runEntry.pauseRequested) {
				state.status = 'paused';
				this.opts.bus.publish('workflow.paused', { id });
			} else {
				state.status = 'completed';
				this.opts.bus.publish('workflow.completed', {
					id,
					agentCalls: state.agentCalls,
				});
			}
		} catch (err) {
			state.status = 'failed';
			state.error = err instanceof Error ? err.message : String(err);
			this.opts.bus.publish('workflow.failed', { id, error: state.error });
		}
		return state;
	}

	pause(id: string): void {
		const run = this.runs.get(id);
		if (run) run.pauseRequested = true;
	}

	snapshot(
		id: string,
	): { state: WorkflowRunState; cache: CallCacheEntry[] } | undefined {
		const run = this.runs.get(id);
		return run ? { state: run.state, cache: [...run.cache] } : undefined;
	}

	private buildApi(
		id: string,
		runEntry: {
			state: WorkflowRunState;
			cache: CallCacheEntry[];
			pauseRequested: boolean;
		},
		getCallIndex: () => number,
	): WorkflowApi {
		const { state, cache } = runEntry;
		const maxCalls = this.opts.maxAgentCalls ?? 200;

		const agent = async (
			prompt: string,
			opts: AgentCallOptions = {},
		): Promise<SubagentResult> => {
			const index = getCallIndex();
			const cached = cache.find(
				(c) => c.index === index && c.prompt === prompt,
			);
			if (cached) {
				this.opts.bus.publish('workflow.agent_replayed', { id, index });
				return cached.result;
			}
			if (state.agentCalls >= maxCalls) {
				throw new Error(`workflow 超过 agent 调用上限 (${maxCalls})`);
			}
			state.agentCalls++;
			const result = await this.opts.executeAgent(prompt, undefined, opts);
			cache.push({ index, prompt, result });
			this.opts.bus.publish('workflow.agent_done', {
				id,
				index,
				label: opts.label,
			});
			return result;
		};

		return {
			args: undefined,
			agent,
			pipeline: async <T, R>(
				items: T[],
				fn: (item: T, index: number) => Promise<R>,
				opts: { concurrency?: number; label?: string } = {},
			): Promise<R[]> => {
				const concurrency = Math.max(1, opts.concurrency ?? 4);
				const results: R[] = new Array(items.length);
				let cursor = 0;
				const workers = Array.from(
					{ length: Math.min(concurrency, items.length) },
					async () => {
						for (;;) {
							const i = cursor++;
							if (i >= items.length) return;
							results[i] = await fn(items[i] as T, i);
						}
					},
				);
				await Promise.all(workers);
				return results;
			},
			constraints: () => this.opts.constraintsProvider?.() ?? {},
			phase: (name: string) => {
				state.phases.push({
					name,
					agentCount: state.agentCalls,
					startedAt: Date.now(),
				});
				this.opts.bus.publish('workflow.phase', { id, name });
			},
		};
	}
}

// AsyncFunction 构造器（顶层 await 支持）
const AsyncFunction = Object.getPrototypeOf(async () => {}).constructor as new (
	...args: string[]
) => (...values: unknown[]) => Promise<unknown>;
