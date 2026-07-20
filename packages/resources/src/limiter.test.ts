import { describe, expect, test } from 'bun:test';
import { AimdController, computeWaitMs, TokenBucket } from './limiter';

describe('TokenBucket', () => {
	test('桶空则拒绝，随时间回填', () => {
		let now = 1000;
		const bucket = new TokenBucket(10, 0.01, () => now);
		expect(bucket.tryTake(10)).toBe(true);
		expect(bucket.tryTake(1)).toBe(false);
		now += 1000;
		expect(bucket.tryTake(10)).toBe(true);
	});
});

describe('AimdController', () => {
	test('429 减半，成功 +1', () => {
		const aimd = new AimdController(8);
		expect(aimd.onRateLimited()).toBe(4);
		expect(aimd.onRateLimited()).toBe(2);
		expect(aimd.onSuccess()).toBe(3);
		expect(aimd.onSuccess()).toBe(4);
	});

	test('remaining <10% 主动降', () => {
		const aimd = new AimdController(8);
		expect(aimd.onRemainingLow(0.05)).toBe(4);
		expect(aimd.onRemainingLow(0.15)).toBe(3);
	});

	test('下限为 min', () => {
		const aimd = new AimdController(2, 1);
		aimd.onRateLimited();
		expect(aimd.onRateLimited()).toBe(1);
	});
});

describe('computeWaitMs', () => {
	test('retry-after 优先', () => {
		const wait = computeWaitMs({
			retryAfterSec: 5,
			attempt: 0,
			jitter: () => 0,
		});
		expect(wait).toBe(5000);
	});

	test('reset 头换算次之', () => {
		const wait = computeWaitMs({
			resetAtMs: 10_000,
			now: 4_000,
			attempt: 0,
			jitter: () => 0,
		});
		expect(wait).toBe(6000);
	});

	test('兜底全抖动指数退避', () => {
		const wait = computeWaitMs({ attempt: 2, baseMs: 500, jitter: () => 0.5 });
		expect(wait).toBe(1000);
	});
});
