import { describe, expect, test } from 'bun:test';
import type { KxenSession } from '@kxen/core';
import { EventBus } from '@kxen/core';
import { ModelResourceManager } from '@kxen/resources';
import { SubagentManager } from './manager';
import type { SubagentSpec } from './types';

function fakeSession(
	summary: string,
	opts?: { failOnPrompt?: boolean },
): KxenSession {
	return {
		inner: {
			agent: {
				state: {
					messages: [
						{ role: 'assistant', content: [{ type: 'text', text: summary }] },
					],
				},
			},
			steer: async () => {},
		},
		prompt: async () => {
			if (opts?.failOnPrompt) throw new Error('prompt failed');
		},
		dispose: async () => {},
	} as unknown as KxenSession;
}

const roles = { execution: { model: 'xai/grok-4.5' } };
const limits = {
	global: { concurrent: 4 },
	providers: { xai: { concurrent: 4 } },
	roles: {},
};

function makeManager(bus = new EventBus()) {
	const mrm = new ModelResourceManager({ roles, limits, bus });
	return new SubagentManager({
		mrm,
		bus,
		createSession: async () => fakeSession('done: 任务完成'),
		toolsFor: () => [],
		repoRoot: '/nonexistent',
	});
}

const spec: SubagentSpec = {
	name: 'execute',
	description: 'd',
	role: 'execution',
};

describe('SubagentManager', () => {
	test('spawn 返回 typed 结果', async () => {
		const mgr = makeManager();
		const handle = await mgr.spawn(spec, '做个任务');
		const result = await handle.result;
		expect(result.summary).toBe('done: 任务完成');
		expect(result.stopReason).toBe('completed');
		expect(result.filesChanged).toEqual([]);
	});

	test('swarm 批量派发', async () => {
		const mgr = makeManager();
		const handles = await mgr.swarm(spec, ['任务1', '任务2', '任务3']);
		const results = await Promise.all(handles.map((h) => h.result));
		expect(results).toHaveLength(3);
		expect(results.every((r) => r.stopReason === 'completed')).toBe(true);
	});

	test('事件进总线', async () => {
		const bus = new EventBus();
		const mgr = makeManager(bus);
		const handle = await mgr.spawn(spec, '任务');
		await handle.result;
		const types = bus.recent().map((e) => e.type);
		expect(types).toContain('subagent.spawn');
		expect(types).toContain('subagent.complete');
	});

	test('steer 转发到会话', async () => {
		const mgr = makeManager();
		const handle = await mgr.spawn(spec, '任务');
		await handle.steer('补充指令');
		await handle.result;
	});

	test('prompt 失败返回 error 结果', async () => {
		const bus = new EventBus();
		const mrm = new ModelResourceManager({ roles, limits, bus });
		const mgr = new SubagentManager({
			mrm,
			bus,
			createSession: async () => fakeSession('', { failOnPrompt: true }),
			toolsFor: () => [],
			repoRoot: '/nonexistent',
		});
		const handle = await mgr.spawn(spec, '任务');
		const result = await handle.result;
		expect(result.stopReason).toBe('error');
		expect(result.error).toContain('prompt failed');
	});
});
