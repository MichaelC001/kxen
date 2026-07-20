import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';

const INDEX_MAX_LINES = 200;
const INDEX_MAX_CHARS = 25_000;

// 自维护索引记忆（analysis/01 C7 / Claude Code MEMORY.md 同型）：
// MEMORY.md 索引（200 行 / 25KB 上限）+ 主题文件，超限部分下次加载即丢
export class FileMemory {
	constructor(private dir: string) {}

	private indexPath(): string {
		return join(this.dir, 'MEMORY.md');
	}

	// 加载索引（截断到上限，与 CC 行为一致）
	loadIndex(): string {
		if (!existsSync(this.indexPath())) return '';
		const raw = readFileSync(this.indexPath(), 'utf8');
		const lines = raw.split('\n').slice(0, INDEX_MAX_LINES);
		return lines.join('\n').slice(0, INDEX_MAX_CHARS);
	}

	// 追加一行索引；超限提示精简（CC v2.1.210 语义）
	appendIndex(line: string): { ok: boolean; warning?: string } {
		mkdirSync(this.dir, { recursive: true });
		const current = this.loadIndex();
		const next = current ? `${current}\n${line}` : line;
		writeFileSync(this.indexPath(), next.endsWith('\n') ? next : `${next}\n`);
		if (
			next.split('\n').length > INDEX_MAX_LINES ||
			next.length > INDEX_MAX_CHARS
		) {
			return {
				ok: true,
				warning: '索引超限，请精简：一行一条，细节移到主题文件',
			};
		}
		return { ok: true };
	}

	writeTopic(name: string, content: string): void {
		const path = join(this.dir, 'topics', `${name}.md`);
		mkdirSync(dirname(path), { recursive: true });
		writeFileSync(path, content);
	}

	readTopic(name: string): string | undefined {
		const path = join(this.dir, 'topics', `${name}.md`);
		return existsSync(path) ? readFileSync(path, 'utf8') : undefined;
	}
}
