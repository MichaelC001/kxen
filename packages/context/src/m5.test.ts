import { describe, expect, test } from 'bun:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { applyHashlineEdits, toHashlines } from '../../tools/src/hashline';
import { FileMemory } from './memory';
import { runMemoryPipeline } from './memory-pipeline';
import { renderTextToFrames } from './snapcompact';
import { StreamRuleEngine } from './ttsr';

describe('hashline', () => {
	test('锚定替换', () => {
		const content = 'aaa\nbbb\nccc';
		const lines = toHashlines(content);
		const out = applyHashlineEdits(content, [
			{ anchor: lines[1]!.hash, action: 'replace', text: 'BBB' },
		]);
		expect(out).toBe('aaa\nBBB\nccc');
	});

	test('锚点丢失拒绝', () => {
		expect(() =>
			applyHashlineEdits('aaa', [{ anchor: 'deadbeef', action: 'delete' }]),
		).toThrow('未找到');
	});

	test('insert_after 与 delete', () => {
		const content = 'a\nb';
		const lines = toHashlines(content);
		const out = applyHashlineEdits(content, [
			{ anchor: lines[0]!.hash, action: 'insert_after', text: 'x' },
			{ anchor: lines[1]!.hash, action: 'delete' },
		]);
		expect(out).toBe('a\nx');
	});
});

describe('ttsr', () => {
	test('正则命中触发规则', () => {
		const engine = new StreamRuleEngine();
		engine.addRule({
			id: 'no-npm',
			pattern: /\bnpm install\b/,
			reminder: '用 bun install',
			enabled: true,
		});
		const hit = engine.check('我打算执行 npm install 来安装依赖');
		expect(hit?.rule.reminder).toBe('用 bun install');
		expect(engine.check('bun install')).toBeUndefined();
	});

	test('disabled 规则不触发', () => {
		const engine = new StreamRuleEngine();
		engine.addRule({ id: 'r', pattern: /x/, reminder: 'm', enabled: false });
		expect(engine.check('x')).toBeUndefined();
	});
});

describe('memory-pipeline', () => {
	test('两阶段提取并写入索引', async () => {
		const dir = mkdtempSync(join(tmpdir(), 'kxen-mem-'));
		try {
			const memory = new FileMemory(dir);
			const r = await runMemoryPipeline(
				[{ id: 's1', content: '用户说项目用 bun 不用 npm' }],
				{
					memory,
					extract: async () => '项目用 bun',
					consolidate: async () => '项目用 bun\n测试用 bun test',
				},
			);
			expect(r.extracted).toBe(1);
			expect(r.consolidated).toBe(true);
			expect(memory.loadIndex()).toContain('项目用 bun');
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	});

	test('NONE 不写入', async () => {
		const dir = mkdtempSync(join(tmpdir(), 'kxen-mem-'));
		try {
			const memory = new FileMemory(dir);
			const r = await runMemoryPipeline([{ id: 's1', content: '闲聊' }], {
				memory,
				extract: async () => 'NONE',
				consolidate: async () => '',
			});
			expect(r.consolidated).toBe(false);
			expect(memory.loadIndex()).toBe('');
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	});
});

describe('snapcompact', () => {
	test('文本渲染成 PNG 帧', async () => {
		const dir = mkdtempSync(join(tmpdir(), 'kxen-snap-'));
		try {
			const text = 'hello snapcompact\n'.repeat(200);
			const frames = await renderTextToFrames(
				text,
				(i) => join(dir, `frame-${i}.png`),
				{
					frameWidth: 512,
					fontSize: 12,
				},
			);
			expect(frames.length).toBeGreaterThan(0);
			const file = Bun.file(frames[0]!.path);
			expect(await file.exists()).toBe(true);
			const bytes = await file.arrayBuffer();
			// PNG 魔数
			expect(new Uint8Array(bytes)[0]).toBe(0x89);
			expect(new Uint8Array(bytes)[1]).toBe(0x50);
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	});
});
