import { describe, expect, test } from 'bun:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { takeSample, writeHeapDump } from './telemetry';

describe('telemetry', () => {
	test('takeSample 返回内存指标', () => {
		const s = takeSample(() => 42);
		expect(s.ts).toBe(42);
		expect(s.rss).toBeGreaterThan(0);
		expect(s.heapUsed).toBeGreaterThan(0);
	});

	test('writeHeapDump 产出快照文件', async () => {
		const dir = mkdtempSync(join(tmpdir(), 'kxen-dump-'));
		try {
			const path = await writeHeapDump(dir, () => 1);
			expect(path).toContain('heap-1.heapsnapshot');
			const file = Bun.file(path);
			expect(await file.exists()).toBe(true);
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	});
});
