import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';

export interface ReadResult {
	content: string;
	totalLines: number;
	offset: number;
	limit: number;
	truncated: boolean;
}

export interface ReadOptions {
	offset?: number;
	limit?: number;
}

// read-before-edit 追踪：编辑前必须读过，且文件未在外部被改（staleness 拒绝）
export class FileTracker {
	private marks = new Map<string, string>();

	private hash(path: string): string {
		const stat = statSync(path);
		return createHash('sha1')
			.update(`${stat.mtimeMs}:${stat.size}`)
			.digest('hex');
	}

	markRead(path: string): void {
		this.marks.set(resolve(path), this.hash(resolve(path)));
	}

	assertFresh(path: string): void {
		const key = resolve(path);
		const known = this.marks.get(key);
		if (!known) {
			throw new Error(`文件 ${key} 未读过，先 read 再 edit`);
		}
		if (known !== this.hash(key)) {
			throw new Error(`文件 ${key} 在读取后被外部修改，重新 read 后再 edit`);
		}
	}
}

export const fileTracker = new FileTracker();

export function readTextFile(path: string, opts: ReadOptions = {}): ReadResult {
	const abs = resolve(path);
	const raw = readFileSync(abs, 'utf8');
	const lines = raw.split('\n');
	const offset = Math.max(1, opts.offset ?? 1);
	const limit = Math.max(1, opts.limit ?? 2000);
	const slice = lines.slice(offset - 1, offset - 1 + limit);
	fileTracker.markRead(abs);
	return {
		content: slice.map((line, i) => `${offset + i}\t${line}`).join('\n'),
		totalLines: lines.length,
		offset,
		limit,
		truncated: offset - 1 + limit < lines.length,
	};
}

export function writeTextFile(path: string, content: string): void {
	const abs = resolve(path);
	mkdirSync(dirname(abs), { recursive: true });
	writeFileSync(abs, content);
	fileTracker.markRead(abs);
}

export function editTextFile(
	path: string,
	oldText: string,
	newText: string,
): void {
	const abs = resolve(path);
	fileTracker.assertFresh(abs);
	const raw = readFileSync(abs, 'utf8');
	const first = raw.indexOf(oldText);
	if (first === -1) {
		throw new Error(`oldText 在 ${abs} 中不存在（注意空白与换行必须完全一致）`);
	}
	if (raw.indexOf(oldText, first + 1) !== -1) {
		throw new Error(`oldText 在 ${abs} 中不唯一，请提供更多上下文`);
	}
	writeFileSync(
		abs,
		raw.slice(0, first) + newText + raw.slice(first + oldText.length),
	);
	fileTracker.markRead(abs);
}
