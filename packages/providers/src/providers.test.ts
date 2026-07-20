import { describe, expect, test } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { readCredential } from './auth-file';
import { getProviderAuth } from './registry';
import './openai-codex';
import './xai';

describe('openai-codex provider', () => {
	test('从 codex auth.json 导入 api_key 型凭证', async () => {
		const dir = mkdtempSync(join(tmpdir(), 'kxen-prov-'));
		try {
			const kxenAuth = join(dir, 'auth.json');
			const auth = getProviderAuth('openai-codex');
			expect(auth).toBeDefined();
			const cred = await auth?.resolve({
				authPath: kxenAuth,
				env: { OPENAI_API_KEY: 'sk-test' },
				cliAuthPaths: { 'openai-codex': join(dir, 'nonexistent.json') },
			});
			expect(cred).toEqual({ type: 'api_key', key: 'sk-test' });
			expect(await readCredential(kxenAuth, 'openai-codex')).toEqual(cred);
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	});
});

describe('xai provider', () => {
	test('从 XAI_API_KEY 导入', async () => {
		const dir = mkdtempSync(join(tmpdir(), 'kxen-prov-'));
		try {
			const kxenAuth = join(dir, 'auth.json');
			const auth = getProviderAuth('xai');
			const cred = await auth?.resolve({
				authPath: kxenAuth,
				env: { XAI_API_KEY: 'xai-test' },
				cliAuthPaths: { xai: join(dir, 'nonexistent.json') },
			});
			expect(cred).toEqual({ type: 'api_key', key: 'xai-test' });
		} finally {
			rmSync(dir, { recursive: true, force: true });
		}
	});
});
