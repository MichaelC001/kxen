import { describe, expect, test } from 'bun:test';
import { EventBus } from '@kxen/core';
import { ModelResourceManager } from './mrm';

const roles = {
	execution: { model: 'xai/grok-4.5', fallbacks: ['kimi/k3'] },
	thinking: { model: 'anthropic/claude-opus' },
};

const limits = {
	global: { concurrent: 2 },
	providers: {
		xai: { concurrent: 1 },
		kimi: { concurrent: 1 },
		anthropic: { concurrent: 1 },
	},
	roles: { execution: { concurrent: 2 }, thinking: { concurrent: 1 } },
};

describe('ModelResourceManager', () => {
	test('acquire / release 与角色路由', async () => {
		const mrm = new ModelResourceManager({ roles, limits });
		const slot = await mrm.acquire({ role: 'execution' });
		expect(slot.model).toBe('xai/grok-4.5');
		expect(slot.providerId).toBe('xai');
		mrm.release(slot, { ok: true });
		expect(mrm.status().global.inFlight).toBe(0);
	});

	test('provider 并发上限触发 fallback', async () => {
		const mrm = new ModelResourceManager({ roles, limits });
		const first = await mrm.acquire({ role: 'execution' });
		const second = await mrm.acquire({ role: 'execution' });
		expect(second.model).toBe('kimi/k3');
		mrm.release(first, { ok: true });
		mrm.release(second, { ok: true });
	});

	test('三层都满时排队，释放后按优先级放行', async () => {
		const mrm = new ModelResourceManager({ roles, limits });
		const a = await mrm.acquire({ role: 'execution' });
		const b = await mrm.acquire({ role: 'thinking' });
		const order: string[] = [];
		const low = mrm.acquire({ role: 'execution', priority: 1 }).then((s) => {
			order.push('low');
			return s;
		});
		const high = mrm.acquire({ role: 'execution', priority: 9 }).then((s) => {
			order.push('high');
			return s;
		});
		mrm.release(a, { ok: true });
		await new Promise((r) => setTimeout(r, 10));
		mrm.release(b, { ok: true });
		const slots = await Promise.all([low, high]);
		expect(order).toEqual(['high', 'low']);
		for (const s of slots) mrm.release(s, { ok: true });
	});

	test('失败 release 标记冷却，冷却中换 provider', async () => {
		const mrm = new ModelResourceManager({ roles, limits });
		const slot = await mrm.acquire({ role: 'execution' });
		mrm.release(slot, { ok: false, error: new Error('429') });
		const next = await mrm.acquire({ role: 'execution' });
		expect(next.model).toBe('kimi/k3');
		mrm.release(next, { ok: true });
	});

	test('事件发布到总线', async () => {
		const bus = new EventBus();
		const mrm = new ModelResourceManager({ roles, limits, bus });
		const slot = await mrm.acquire({ role: 'thinking' });
		mrm.release(slot, { ok: true });
		const types = bus.recent().map((e) => e.type);
		expect(types).toContain('mrm.acquired');
		expect(types).toContain('mrm.released');
	});
});
