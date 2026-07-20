import { createHash } from 'node:crypto';

function lineHash(line: string): string {
	return createHash('sha1').update(line.trimEnd()).digest('hex').slice(0, 8);
}

export interface HashlineRef {
	hash: string;
	text: string;
}

export interface HashlineEdit {
	anchor: string;
	action: 'replace' | 'insert_after' | 'delete';
	text?: string;
}

// hashline（OMP 同型）：用内容 hash 锚定行，锚不上即拒绝，消灭 stale 编辑
export function toHashlines(content: string): HashlineRef[] {
	return content.split('\n').map((text) => ({ hash: lineHash(text), text }));
}

export function applyHashlineEdits(
	content: string,
	edits: HashlineEdit[],
): string {
	const lines = content.split('\n');
	const out: string[] = [];
	let cursor = 0;
	for (const edit of edits) {
		const idx = lines.findIndex(
			(line, i) => i >= cursor && lineHash(line) === edit.anchor,
		);
		if (idx === -1) {
			throw new Error(
				`hashline 锚点 ${edit.anchor} 未找到（文件可能已被外部修改）`,
			);
		}
		while (cursor < idx) {
			out.push(lines[cursor] as string);
			cursor++;
		}
		const anchored = lines[cursor] as string;
		if (edit.action === 'replace') {
			out.push(edit.text ?? '');
			cursor++;
		} else if (edit.action === 'insert_after') {
			out.push(anchored);
			out.push(edit.text ?? '');
			cursor++;
		} else {
			cursor++;
		}
	}
	while (cursor < lines.length) {
		out.push(lines[cursor] as string);
		cursor++;
	}
	return out.join('\n');
}
