import { beforeEach, describe, expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { editTextFile, FileTracker, readTextFile, writeTextFile } from './fs';

describe('fs', () => {
	let dir: string;
	beforeEach(() => {
		dir = mkdtempSync(join(tmpdir(), 'kxen-fs-'));
		return () => rmSync(dir, { recursive: true, force: true });
	});

	test('read 带行号且标记已读', () => {
		const path = join(dir, 'a.txt');
		writeFileSync(path, 'l1\nl2\nl3');
		const tracker = new FileTracker();
		const r = readTextFile(path, {});
		expect(r.content).toContain('1\tl1');
		expect(r.totalLines).toBe(3);
	});

	test('edit 唯一性校验', () => {
		const path = join(dir, 'b.txt');
		writeFileSync(path, 'foo bar foo');
		readTextFile(path, {});
		expect(() => editTextFile(path, 'foo', 'baz')).toThrow('不唯一');
	});

	test('edit 未读拒绝', () => {
		const path = join(dir, 'c.txt');
		writeFileSync(path, 'hello');
		const tracker = new FileTracker();
		expect(() => editTextFile(path, 'hello', 'world')).toThrow();
	});

	test('edit 外部修改后 staleness 拒绝', async () => {
		const path = join(dir, 'd.txt');
		writeFileSync(path, 'hello');
		readTextFile(path, {});
		await new Promise((r) => setTimeout(r, 10));
		writeFileSync(path, 'changed outside');
		expect(() => editTextFile(path, 'hello', 'world')).toThrow('外部修改');
	});

	test('write 后 edit 正常工作流', () => {
		const path = join(dir, 'e.txt');
		writeTextFile(path, 'hello world');
		editTextFile(path, 'world', 'kxen');
		const r = readTextFile(path, {});
		expect(r.content).toContain('hello kxen');
	});

	test('read offset/limit 截断', () => {
		const path = join(dir, 'f.txt');
		writeFileSync(path, '1\n2\n3\n4\n5');
		const r = readTextFile(path, { offset: 2, limit: 2 });
		expect(r.content).toBe('2\t2\n3\t3');
		expect(r.truncated).toBe(true);
	});
});
