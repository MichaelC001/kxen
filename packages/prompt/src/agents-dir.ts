import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { basename, join, relative } from 'node:path';
import { parse as parseYaml } from 'yaml';

export type AgentsDocType =
	| 'rule'
	| 'reference'
	| 'skill'
	| 'command'
	| 'agent'
	| 'workflow';

export interface AgentsDoc {
	path: string;
	type: AgentsDocType;
	title?: string;
	description?: string;
	priority?: 'high' | 'normal' | 'low';
	applyTo?: string[];
	alwaysApply?: boolean;
	roles?: string[];
	tags?: string[];
	body: string;
}

// OKF frontmatter 解析（design/09）：宽松消费，未知字段容忍
export function parseAgentsDoc(
	path: string,
	raw: string,
): AgentsDoc | undefined {
	const match = raw.match(/^---\n([\s\S]*?)\n---\n?([\s\S]*)$/);
	if (!match) return undefined;
	const fm = parseYaml(match[1] ?? '') as Record<string, unknown>;
	if (!fm?.type) return undefined;
	return {
		path,
		type: fm.type as AgentsDocType,
		title: fm.title as string | undefined,
		description: fm.description as string | undefined,
		priority: fm.priority as AgentsDoc['priority'],
		applyTo: fm.applyTo as string[] | undefined,
		alwaysApply: fm.alwaysApply as boolean | undefined,
		roles: fm.roles as string[] | undefined,
		tags: fm.tags as string[] | undefined,
		body: (match[2] ?? '').trim(),
	};
}

// 递归扫描 .agents/ 目录
export function loadAgentsDir(dir: string): AgentsDoc[] {
	if (!existsSync(dir)) return [];
	const docs: AgentsDoc[] = [];
	const walk = (current: string) => {
		for (const entry of readdirSync(current)) {
			const full = join(current, entry);
			if (statSync(full).isDirectory()) {
				walk(full);
			} else if (
				entry.endsWith('.md') &&
				entry !== 'index.md' &&
				entry !== 'log.md'
			) {
				const doc = parseAgentsDoc(full, readFileSync(full, 'utf8'));
				if (doc) docs.push(doc);
			}
		}
	};
	walk(dir);
	return docs;
}

export interface InjectedRules {
	always: AgentsDoc[];
	scoped: AgentsDoc[];
}

// 加载语义（design/09 矩阵）：rule 按 alwaysApply / applyTo 分流；reference 永不自动注入
export function selectRules(
	docs: AgentsDoc[],
	touchedPath?: string,
): InjectedRules {
	const rules = docs.filter((d) => d.type === 'rule');
	return {
		always: rules.filter((d) => d.alwaysApply === true),
		scoped: rules.filter((d) => {
			if (d.alwaysApply) return false;
			if (!d.applyTo || !touchedPath) return false;
			return d.applyTo.some((glob) => matchGlob(touchedPath, glob));
		}),
	};
}

function matchGlob(path: string, glob: string): boolean {
	const pattern = glob
		.replace(/[.+^${}()|[\]\\]/g, '\\$&')
		.replace(/\*\*/g, ' DOUBLESTAR ')
		.replace(/\*/g, '[^/]*')
		.replace(/ DOUBLESTAR /g, '.*');
	return (
		new RegExp(`^${pattern}$`).test(path) ||
		new RegExp(`(^|/)${pattern}`).test(path)
	);
}

// index.md 生成（禁止手写，design/09）
export function generateIndex(dir: string, docs: AgentsDoc[]): string {
	const groups = new Map<string, AgentsDoc[]>();
	for (const doc of docs) {
		const list = groups.get(doc.type) ?? [];
		list.push(doc);
		groups.set(doc.type, list);
	}
	const lines: string[] = ['# .agents 索引（自动生成，禁止手写）', ''];
	for (const [type, list] of [...groups.entries()].sort()) {
		lines.push(`## ${type}`);
		for (const doc of list) {
			const rel = relative(dir, doc.path);
			lines.push(
				`- [${doc.title ?? basename(doc.path, '.md')}](${rel}) - ${doc.description ?? ''}`,
			);
		}
		lines.push('');
	}
	return lines.join('\n');
}
