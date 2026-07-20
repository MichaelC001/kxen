import { describe, expect, test } from 'bun:test';
import { EventBus } from './events';

describe('EventBus', () => {
	test('发布与订阅', () => {
		const bus = new EventBus({ now: () => 1 });
		const seen: string[] = [];
		bus.subscribe({}, (e) => {
			seen.push(e.type);
		});
		bus.publish('a', {});
		bus.publish('b', {});
		expect(seen).toEqual(['a', 'b']);
	});

	test('超出容量 drop-oldest', () => {
		const bus = new EventBus({ capacity: 3, now: () => 1 });
		for (let i = 0; i < 5; i++) bus.publish(`e${i}`, {});
		expect(bus.size).toBe(3);
		expect(bus.recent().map((e) => e.type)).toEqual(['e2', 'e3', 'e4']);
	});

	test('unsub 后不再收到', () => {
		const bus = new EventBus();
		let count = 0;
		const unsub = bus.subscribe({}, () => {
			count++;
		});
		bus.publish('a', {});
		unsub();
		bus.publish('b', {});
		expect(count).toBe(1);
	});

	test('unsubscribeAll 按 owner 清理', () => {
		const bus = new EventBus();
		const owner = {};
		let count = 0;
		bus.subscribe(owner, () => {
			count++;
		});
		bus.subscribe({}, () => {
			count++;
		});
		bus.unsubscribeAll(owner);
		bus.publish('a', {});
		expect(count).toBe(1);
	});

	test('慢消费者积压超限被断开并记事件', async () => {
		const bus = new EventBus({ slowConsumerLimit: 1, now: () => 1 });
		let gate: (() => void) | undefined;
		bus.subscribe({}, async () => {
			await new Promise<void>((resolve) => {
				gate = resolve;
			});
		});
		bus.publish('a', {});
		bus.publish('b', {});
		bus.publish('c', {});
		gate?.();
		await Promise.resolve();
		const types = bus.recent().map((e) => e.type);
		expect(types).toContain('eventbus.consumer_dropped');
	});
});
