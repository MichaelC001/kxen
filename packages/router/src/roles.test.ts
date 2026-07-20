import { describe, expect, test } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
	loadRoles,
	parseThinkingSuffix,
	providerOf,
	resolveRole,
} from './roles';

describe('roles', () => {
	test('parseThinkingSuffix 分离模型与档位', () => {
		expect(parseThinkingSuffix('anthropic/claude-opus:high')).toEqual({
			model: 'anthropic/claude-opus',
			thinkingLevel: 'high',
		});
		expect(parseThinkingSuffix('anthropic/claude-opus')).toEqual({
			model: 'anthropic/claude-opus',
		});
	});

	test('providerOf', () => {
		expect(providerOf('anthropic/claude-opus')).toBe('anthropic');
		expect(providerOf('kimi')).toBe('kimi');
	});

	test('loadRoles 读 TOML 并解析角色', () => {
		const dir = mkdtempSync(join(tmpdir(), 'kxen-roles-'));
		try {
			const path = join(dir, 'roles.toml');
			writeFileSync(
				path,
				`
[roles.thinking]
model = "anthropic/claude-opus:high"
fallbacks = ["kimi/k3"]
`,
			);
			const roles = loadRoles(path);
			const r = resolveRole(roles, 'thinking');
			expect(r?.primary).toEqual({
				model: 'anthropic/claude-opus',
				thinkingLevel: 'high',
			});
			expect(r?.chain).toEqual([{ model: 'kimi/k3' }]);
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	});

	test('resolveRole 未知角色返回 undefined', () => {
		expect(resolveRole({}, 'nope')).toBeUndefined();
	});
});
