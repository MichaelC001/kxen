import { describe, expect, test } from 'bun:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { clearToolResults } from './clearing';
import { FileMemory } from './memory';

describe('clearing', () => {
	test('最近 N 轮保留，旧结果截断', () => {
		const big = 'x'.repeat(5000);
		const entries = [
			{ kind: 'message' },
			{ kind: 'toolResult', payload: big },
			{ kind: 'toolResult', payload: big },
			{ kind: 'toolResult', payload: big },
			{ kind: 'toolResult', payload: big },
		];
		const cleared = clearToolResults(entries, {
			keepRecent: 2,
			maxChars: 1000,
		});
		expect(cleared[1]?.payload?.length).toBeLessThan(1100);
		expect(cleared[2]?.payload?.length).toBeLessThan(1100);
		expect(cleared[3]?.payload).toBe(big);
		expect(cleared[4]?.payload).toBe(big);
	});

	test('useless 结果替换为占位符', () => {
		const entries = [{ kind: 'toolResult', payload: 'big', useless: true }];
		const cleared = clearToolResults(entries, { keepRecent: 0 });
		expect(cleared[0]?.payload).toContain('cleared');
	});

	test('数组 payload 只截文本段', () => {
		const entries = [
			{
				kind: 'toolResult',
				payload: [
					{ kind: 'text', text: 'y'.repeat(3000) },
					{ kind: 'image', data: 'base64' },
				],
			},
		];
		const cleared = clearToolResults(entries, { keepRecent: 0, maxChars: 500 });
		const payload = cleared[0]?.payload as Array<{
			kind?: string;
			text?: string;
		}>;
		expect(payload[0]?.text?.length).toBeLessThan(600);
		expect(payload[1]?.kind).toBe('image');
	});
});

describe('FileMemory', () => {
	test('索引读写与主题文件', () => {
		const dir = mkdtempSync(join(tmpdir(), 'kxen-mem-'));
		try {
			const mem = new FileMemory(dir);
			expect(mem.loadIndex()).toBe('');
			mem.appendIndex('构建用 bun run build');
			mem.appendIndex('测试用 bun test');
			expect(mem.loadIndex()).toContain('bun run build');
			mem.writeTopic('build', '详细构建说明');
			expect(mem.readTopic('build')).toBe('详细构建说明');
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	});

	test('超限给出警告', () => {
		const dir = mkdtempSync(join(tmpdir(), 'kxen-mem-'));
		try {
			const mem = new FileMemory(dir);
			let warned = false;
			for (let i = 0; i < 210; i++) {
				const r = mem.appendIndex(`条目 ${i}`);
				if (r.warning) warned = true;
			}
			expect(warned).toBe(true);
			// 加载时截断到 200 行
			expect(mem.loadIndex().split('\n').length).toBeLessThanOrEqual(200);
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	});
});
