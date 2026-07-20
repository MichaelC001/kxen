import { describe, expect, test } from 'bun:test';
import { EventBus } from '@kxen/core';
import { BudgetAccount } from './budget';

describe('BudgetAccount', () => {
	test('80% 与 95% 水位各触发一次', () => {
		const bus = new EventBus();
		const account = new BudgetAccount({ tokens: 1000 }, bus);
		account.record({ tokens: 850 });
		account.record({ tokens: 50 });
		account.record({ tokens: 100 });
		const types = bus.recent().map((e) => e.type);
		expect(types).toContain('budget.warning');
		expect(types).toContain('budget.critical');
		expect(types.filter((t) => t === 'budget.warning')).toHaveLength(1);
	});

	test('exhausted 判断', () => {
		const account = new BudgetAccount({ tokens: 100 });
		account.record({ tokens: 100 });
		expect(account.exhausted()).toBe(true);
	});

	test('无限额时水位为 0', () => {
		const account = new BudgetAccount({});
		account.record({ tokens: 999999 });
		expect(account.watermark()).toBe(0);
	});
});
