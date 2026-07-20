export interface ToolResultLike {
	kind: 'toolResult' | 'message' | string;
	callId?: string;
	payload?: string | Array<{ kind?: string; text?: string; type?: string }>;
	useless?: boolean;
	[key: string]: unknown;
}

export interface ClearingOptions {
	// 最近 N 轮原样保留
	keepRecent?: number;
	// 单个结果最大字符（head+tail 截断，0.6 head 比，OMP 同型）
	maxChars?: number;
	// 占位文案
	placeholder?: string;
}

function trimHeadTail(text: string, maxChars: number): string {
	if (text.length <= maxChars) return text;
	const head = Math.floor(maxChars * 0.6);
	const tail = maxChars - head;
	return `${text.slice(0, head)}\n... [trimmed ${text.length - maxChars} chars] ...\n${text.slice(-tail)}`;
}

// tool-result clearing（analysis/01 C1）：旧的可重取结果替换为占位符，保留调用记录
export function clearToolResults<T extends ToolResultLike>(
	entries: T[],
	opts: ClearingOptions = {},
): T[] {
	const keepRecent = opts.keepRecent ?? 3;
	const maxChars = opts.maxChars ?? 2000;
	const placeholder = opts.placeholder ?? '[output cleared: 结果可重新获取]';

	const resultIndexes: number[] = [];
	for (let i = 0; i < entries.length; i++) {
		if (entries[i]?.kind === 'toolResult') resultIndexes.push(i);
	}
	const protectedFrom = resultIndexes.length - keepRecent;

	return entries.map((entry, i) => {
		if (entry.kind !== 'toolResult') return entry;
		const position = resultIndexes.indexOf(i);
		if (position >= protectedFrom) return entry;
		if (entry.useless) {
			return { ...entry, payload: placeholder };
		}
		if (typeof entry.payload === 'string') {
			return { ...entry, payload: trimHeadTail(entry.payload, maxChars) };
		}
		if (Array.isArray(entry.payload)) {
			return {
				...entry,
				payload: entry.payload.map((p) => {
					if ((p.kind === 'text' || p.type === 'text') && p.text) {
						return { ...p, text: trimHeadTail(p.text, maxChars) };
					}
					return p;
				}),
			};
		}
		return entry;
	});
}
