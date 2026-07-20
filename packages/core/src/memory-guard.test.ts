import { describe, expect, test } from 'bun:test';
import { EventBus } from './events';
import { MemoryGuard } from './memory-guard';

describe('MemoryGuard', () => {
	test('正常水位返回 ok', () => {
		const guard = new MemoryGuard({ warnBytes: 100, criticalBytes: 200 });
		expect(
			guard.check({ ts: 0, rss: 0, heapUsed: 50, heapTotal: 0, external: 0 }),
		).toEqual(['ok']);
	});

	test('warn 触发 display 驱逐', () => {
		const bus = new EventBus();
		const guard = new MemoryGuard({ warnBytes: 100, criticalBytes: 200, bus });
		const actions = guard.check({
			ts: 0,
			rss: 0,
			heapUsed: 150,
			heapTotal: 0,
			external: 0,
		});
		expect(actions).toContain('evict_display');
		expect(bus.recent().map((e) => e.type)).toContain('memory.warn');
	});

	test('critical 按 E1 顺序全量动作', () => {
		const guard = new MemoryGuard({ warnBytes: 100, criticalBytes: 200 });
		const actions = guard.check({
			ts: 0,
			rss: 0,
			heapUsed: 250,
			heapTotal: 0,
			external: 0,
		});
		expect(actions).toEqual([
			'evict_display',
			'request_compaction',
			'reject_new',
		]);
	});
});
