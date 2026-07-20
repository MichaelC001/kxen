import { describe, expect, test } from 'bun:test';
import { EventBus } from '@kxen/core';
import { GoalEngine } from './engine';

function makeEngine() {
	const bus = new EventBus();
	return { bus, engine: new GoalEngine({ bus }) };
}

const contract = {
	objective: '修复全部失败测试',
	completionCriteria: 'bun test 退出码为 0',
};

describe('GoalEngine', () => {
	test('缺 completionCriteria 拒绝创建', () => {
		const { engine } = makeEngine();
		expect(() =>
			engine.create({ objective: 'x', completionCriteria: '' }),
		).toThrow('contract');
	});

	test('单活跃：激活新 goal 时旧的回队列', () => {
		const { engine } = makeEngine();
		const g1 = engine.create(contract, 1);
		const g2 = engine.create(contract, 0);
		engine.activate(g1.id);
		engine.activate(g2.id);
		expect(engine.get(g1.id)?.status).toBe('queued');
		expect(engine.activeGoal()?.id).toBe(g2.id);
	});

	test('blocked 三次规则：同一原因第三次才 blocked', () => {
		const { engine } = makeEngine();
		const g = engine.create(contract);
		engine.activate(g.id);
		engine.applyTurn(g.id, { summary: '尝试1', blockedReason: '凭证缺失' });
		engine.applyTurn(g.id, { summary: '尝试2', blockedReason: '凭证缺失' });
		expect(engine.get(g.id)?.status).toBe('active');
		engine.applyTurn(g.id, { summary: '尝试3', blockedReason: '凭证缺失' });
		expect(engine.get(g.id)?.status).toBe('blocked');
	});

	test('不同原因重置计数', () => {
		const { engine } = makeEngine();
		const g = engine.create(contract);
		engine.activate(g.id);
		engine.applyTurn(g.id, { summary: '1', blockedReason: '原因A' });
		engine.applyTurn(g.id, { summary: '2', blockedReason: '原因B' });
		engine.applyTurn(g.id, { summary: '3', blockedReason: '原因A' });
		expect(engine.get(g.id)?.status).toBe('active');
	});

	test('终态阻塞当轮即 blocked', () => {
		const { engine } = makeEngine();
		const g = engine.create(contract);
		engine.activate(g.id);
		engine.applyTurn(g.id, {
			summary: 'x',
			blockedReason: '目标矛盾',
			terminal: true,
		});
		expect(engine.get(g.id)?.status).toBe('blocked');
	});

	test('预算 turns 超限置 budget_limited', () => {
		const { engine } = makeEngine();
		const g = engine.create({ ...contract, budget: { turns: 2 } });
		engine.activate(g.id);
		engine.applyTurn(g.id, { summary: '1' });
		engine.applyTurn(g.id, { summary: '2' });
		expect(engine.get(g.id)?.status).toBe('budget_limited');
	});

	test('complete 需要证据且激活下一个', () => {
		const { engine } = makeEngine();
		const g1 = engine.create(contract, 9);
		const g2 = engine.create(contract, 1);
		engine.activate(g1.id);
		expect(() => engine.complete(g1.id, '')).toThrow('证据');
		engine.complete(g1.id, 'bun test 全绿');
		expect(engine.get(g1.id)?.status).toBe('complete');
		expect(engine.activeGoal()?.id).toBe(g2.id);
	});

	test('pause / resume', () => {
		const { engine } = makeEngine();
		const g = engine.create(contract);
		engine.activate(g.id);
		engine.pause(g.id);
		expect(engine.get(g.id)?.status).toBe('paused');
		engine.resume(g.id);
		expect(engine.get(g.id)?.status).toBe('active');
	});
});
