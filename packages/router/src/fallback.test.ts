import { describe, expect, test } from 'bun:test';
import { HealthTracker, selectFromChain } from './fallback';

describe('fallback', () => {
	test('跳过冷却中的 provider', () => {
		const tracker = new HealthTracker(60_000);
		tracker.markFailure('anthropic', 1000);
		const chain = ['anthropic/a', 'kimi/b', 'openai/c'];
		const selected = selectFromChain(chain, (m) => {
			const provider = m.split('/')[0] ?? m;
			return !tracker.isCoolingDown(provider, 2000);
		});
		expect(selected).toBe('kimi/b');
	});

	test('cooldown 到期恢复资格', () => {
		const tracker = new HealthTracker(60_000);
		tracker.markFailure('anthropic', 1000);
		expect(tracker.isCoolingDown('anthropic', 2000)).toBe(true);
		expect(tracker.isCoolingDown('anthropic', 62_000)).toBe(false);
	});

	test('成功后清零失败计数', () => {
		const tracker = new HealthTracker();
		tracker.markFailure('anthropic', 1000);
		tracker.markSuccess('anthropic');
		expect(tracker.state('anthropic').consecutiveFailures).toBe(0);
	});
});
