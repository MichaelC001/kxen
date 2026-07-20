import { describe, expect, test } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { CheckpointStore } from './checkpoint';

describe('CheckpointStore', () => {
	test('创建快照并回滚', async () => {
		const dir = mkdtempSync(join(tmpdir(), 'kxen-ckpt-'));
		try {
			writeFileSync(join(dir, 'a.txt'), 'v1');
			const store = new CheckpointStore(dir);
			const commit = await store.create(dir, 'v1 快照');
			expect(commit).toBeTruthy();

			writeFileSync(join(dir, 'a.txt'), 'v2 改坏了');
			await store.restore(dir, commit);
			const content = await Bun.file(join(dir, 'a.txt')).text();
			expect(content).toBe('v1');
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	});
});
