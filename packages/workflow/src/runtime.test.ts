import { describe, expect, test } from 'bun:test';
import { EventBus } from '@kxen/core';
import type { SubagentResult } from '@kxen/subagent';
import { WorkflowRuntime } from './runtime';

function makeRuntime(opts?: { count?: { n: number } }) {
	const bus = new EventBus();
	const counter = opts?.count ?? { n: 0 };
	const runtime = new WorkflowRuntime({
		bus,
		executeAgent: async (prompt) => {
			counter.n++;
			return {
				summary: `结果:${prompt.slice(0, 20)}`,
				filesChanged: [],
				stopReason: 'completed',
			} as SubagentResult;
		},
		constraintsProvider: () => ({ global: { inFlight: 1, max: 16 } }),
		maxAgentCalls: 50,
	});
	return { bus, runtime, counter };
}

describe('WorkflowRuntime', () => {
	test('脚本执行 agent/pipeline/phase', async () => {
		const { runtime, counter } = makeRuntime();
		const state = await runtime.run(`
			phase('audit')
			const found = await agent('列出问题文件')
			const audits = await pipeline([1, 2, 3], async (i) => (await agent('审计文件' + i)).summary)
			return { found: found.summary, audits }
		`);
		expect(state.status).toBe('completed');
		expect(counter.n).toBe(4);
		expect(state.phases).toHaveLength(1);
		const result = state.result as { audits: string[] };
		expect(result.audits).toHaveLength(3);
	});

	test('constraints() 返回资源快照', async () => {
		const { runtime } = makeRuntime();
		const state = await runtime.run(`return constraints()`);
		expect((state.result as { global: { max: number } }).global.max).toBe(16);
	});

	test('resume 时已完成的调用直接回放不重跑', async () => {
		const { runtime, counter } = makeRuntime();
		const first =
			(await runtime
				.run(`
			const a = await agent('任务A')
			throw new Error('中断')
		`)
				.catch(() => undefined)) ??
			(await runtime.run(`const a = await agent('任务A'); return a`));
		const snap = runtime.snapshot(first.id);
		expect(snap).toBeDefined();
		const before = counter.n;
		const resumed = await runtime.run(
			`const a = await agent('任务A'); return a`,
			undefined,
			{ id: first.id, cache: snap!.cache },
		);
		expect(resumed.status).toBe('completed');
		expect(counter.n).toBe(before);
	});

	test('超过 agent 上限报错', async () => {
		const bus = new EventBus();
		const runtime = new WorkflowRuntime({
			bus,
			executeAgent: async () => ({
				summary: 'x',
				filesChanged: [],
				stopReason: 'completed',
			}),
			maxAgentCalls: 2,
		});
		const state = await runtime.run(`
			await agent('1')
			await agent('2')
			await agent('3')
		`);
		expect(state.status).toBe('failed');
		expect(state.error).toContain('上限');
	});

	test('脚本异常标记 failed', async () => {
		const { runtime } = makeRuntime();
		const state = await runtime.run(`throw new Error('boom')`);
		expect(state.status).toBe('failed');
		expect(state.error).toBe('boom');
	});
});
