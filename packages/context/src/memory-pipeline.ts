import type { FileMemory } from './memory';

export interface SessionTranscript {
	id: string;
	content: string;
}

export interface MemoryPipelineDeps {
	// 用便宜模型做提取（tiny 角色）
	extract: (prompt: string) => Promise<string>;
	consolidate: (prompt: string) => Promise<string>;
	memory: FileMemory;
}

const EXTRACT_PROMPT = (content: string) =>
	`从以下会话中提取可长期保存的事实（技术决策、反复出现的问题、工作流偏好），一行一条，没有就回答 NONE：\n\n${content}`;

const CONSOLIDATE_PROMPT = (entries: string[]) =>
	`把以下多份提取结果合并去重为精炼的长期记忆索引（一行一条，最多 50 行）：\n\n${entries.join('\n---\n')}`;

// 离线记忆管线（OMP Hindsight 同型）：两阶段提取 + 合并，写进 FileMemory
export async function runMemoryPipeline(
	sessions: SessionTranscript[],
	deps: MemoryPipelineDeps,
): Promise<{ extracted: number; consolidated: boolean }> {
	const entries: string[] = [];
	for (const session of sessions) {
		const out = await deps.extract(EXTRACT_PROMPT(session.content));
		if (out.trim() && out.trim() !== 'NONE') entries.push(out.trim());
	}
	if (entries.length === 0) return { extracted: 0, consolidated: false };
	const summary = await deps.consolidate(CONSOLIDATE_PROMPT(entries));
	for (const line of summary.split('\n').filter((l) => l.trim())) {
		deps.memory.appendIndex(line);
	}
	return { extracted: entries.length, consolidated: true };
}
