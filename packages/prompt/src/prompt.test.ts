import { describe, expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
	generateIndex,
	loadAgentsDir,
	parseAgentsDoc,
	selectRules,
} from './agents-dir';
import { composePrompt, renderConditionals, renderTemplate } from './composer';

const RULE_MD = `---
type: rule
title: 构建命令
description: bun 工程命令
priority: high
alwaysApply: true
---
用 bun，不用 npm。
`;

const SCOPED_MD = `---
type: rule
title: TS 规则
applyTo: ["src/**/*.ts"]
---
TS 文件用 strict。
`;

const REF_MD = `---
type: reference
title: API 文档
---
外部 API 说明。
`;

describe('agents-dir', () => {
	test('parseAgentsDoc 解析 frontmatter', () => {
		const doc = parseAgentsDoc('r.md', RULE_MD);
		expect(doc?.type).toBe('rule');
		expect(doc?.alwaysApply).toBe(true);
		expect(doc?.body).toContain('用 bun');
	});

	test('无 frontmatter 返回 undefined', () => {
		expect(parseAgentsDoc('x.md', '# 普通 markdown')).toBeUndefined();
	});

	test('selectRules 分流 alwaysApply / applyTo / reference', () => {
		const docs = [
			parseAgentsDoc('a.md', RULE_MD)!,
			parseAgentsDoc('b.md', SCOPED_MD)!,
			parseAgentsDoc('c.md', REF_MD)!,
		];
		const { always, scoped } = selectRules(docs, 'src/tools/exec.ts');
		expect(always).toHaveLength(1);
		expect(scoped).toHaveLength(1);
		expect(scoped[0]?.title).toBe('TS 规则');
		const none = selectRules(docs, 'other/file.md');
		expect(none.scoped).toHaveLength(0);
		expect(docs.filter((d) => d.type === 'reference')).toHaveLength(1);
	});

	test('loadAgentsDir 递归扫描并生成 index', () => {
		const dir = mkdtempSync(join(tmpdir(), 'kxen-agents-'));
		try {
			mkdirSync(join(dir, 'rules'), { recursive: true });
			mkdirSync(join(dir, 'references'), { recursive: true });
			writeFileSync(join(dir, 'rules', 'a.md'), RULE_MD);
			writeFileSync(join(dir, 'references', 'b.md'), REF_MD);
			const docs = loadAgentsDir(dir);
			expect(docs).toHaveLength(2);
			const index = generateIndex(dir, docs);
			expect(index).toContain('## rule');
			expect(index).toContain('## reference');
			expect(index).toContain('构建命令');
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	});
});

describe('composer', () => {
	test('renderTemplate 插值', () => {
		expect(
			renderTemplate('用 {{tools.exec}} 执行', { 'tools.exec': 'exec' }),
		).toBe('用 exec 执行');
	});

	test('renderConditionals 条件段', () => {
		const t = '{{#if tools.exec}}有 exec{{/if}}{{#if tools.lsp}}有 lsp{{/if}}';
		expect(renderConditionals(t, { 'tools.exec': true })).toBe('有 exec');
	});

	test('composePrompt 按优先级拼接', () => {
		const out = composePrompt([
			{ id: 'b', priority: 2, content: '第二段' },
			{ id: 'a', priority: 1, content: '第一段' },
		]);
		expect(out).toBe('第一段\n\n第二段');
	});
});
