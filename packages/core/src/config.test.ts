import { describe, expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { loadConfig } from './config';

describe('loadConfig', () => {
	test('四层合并：全局 -> 项目 -> 运行时覆盖', () => {
		const dir = mkdtempSync(join(tmpdir(), 'kxen-cfg-'));
		try {
			const globalPath = join(dir, 'global.toml');
			writeFileSync(
				globalPath,
				`
[roles.thinking]
model = "anthropic/claude-opus"
fallbacks = ["openai/gpt"]
[limits.global]
concurrent = 16
`,
			);
			const projectDir = join(dir, 'repo');
			mkdirSync(join(projectDir, '.agents'), { recursive: true });
			writeFileSync(
				join(projectDir, '.agents', 'config.toml'),
				`
[roles.thinking]
model = "kimi/k3"
`,
			);
			const cfg = loadConfig({
				globalPath,
				projectDir,
				overrides: { limits: { global: { concurrent: 8 } } },
			});
			expect(cfg.roles.thinking?.model).toBe('kimi/k3');
			expect(cfg.roles.thinking?.fallbacks).toEqual(['openai/gpt']);
			expect(cfg.limits.global?.concurrent).toBe(8);
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	});

	test('数组整层替换不合并', () => {
		const dir = mkdtempSync(join(tmpdir(), 'kxen-cfg-'));
		try {
			const globalPath = join(dir, 'global.toml');
			writeFileSync(
				globalPath,
				'roles = { t = { model = "a", fallbacks = ["x", "y"] } }\n',
			);
			const cfg = loadConfig({
				globalPath,
				overrides: { roles: { t: { model: 'a', fallbacks: ['z'] } } },
			});
			expect(cfg.roles.t?.fallbacks).toEqual(['z']);
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	});

	test('文件不存在时返回默认', () => {
		const cfg = loadConfig({ globalPath: '/nonexistent/config.toml' });
		expect(cfg.roles).toEqual({});
	});
});
