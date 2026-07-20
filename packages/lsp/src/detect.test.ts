import { describe, expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { detectLanguages, detectServers } from './detect';

describe('lsp detect', () => {
	test('按标记文件识别语言', () => {
		const dir = mkdtempSync(join(tmpdir(), 'kxen-lsp-'));
		try {
			writeFileSync(join(dir, 'package.json'), '{}');
			writeFileSync(join(dir, 'go.mod'), 'module x');
			const langs = detectLanguages(dir);
			expect(langs).toContain('typescript');
			expect(langs).toContain('go');
			expect(langs).not.toContain('rust');
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	});

	test('tsconfig 识别为 typescript', () => {
		const dir = mkdtempSync(join(tmpdir(), 'kxen-lsp-'));
		try {
			writeFileSync(join(dir, 'tsconfig.json'), '{}');
			expect(detectLanguages(dir)).toContain('typescript');
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	});

	test('detectServers 只返回 PATH 上存在的 server', async () => {
		const dir = mkdtempSync(join(tmpdir(), 'kxen-lsp-'));
		try {
			writeFileSync(join(dir, 'Cargo.toml'), '[package]');
			const servers = await detectServers(dir);
			// rust-analyzer 在本机可能不存在；断言返回的都是有 binaryPath 的
			for (const s of servers) {
				expect(s.binaryPath).toBeTruthy();
			}
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	});
});
