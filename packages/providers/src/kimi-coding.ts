import { existsSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { httpFetch, readJson } from '@kxen/core';
import { type Credential, readCredential, writeCredential } from './auth-file';
import { registerProviderAuth } from './registry';

const KIMI_MODELS_URL = 'https://api.kimi.com/coding/v1/models';

interface KimiCliCredentials {
	access_token?: string;
	refresh_token?: string;
	expires_at?: number;
}

// kimi CLI 的 OAuth 凭证（~/.kimi-code/credentials/kimi-code.json）
function readKimiCliCredentials(credPath: string): Credential | undefined {
	if (!existsSync(credPath)) return undefined;
	try {
		const parsed = JSON.parse(
			readFileSync(credPath, 'utf8'),
		) as KimiCliCredentials;
		if (parsed.access_token) {
			return {
				type: 'oauth',
				access: parsed.access_token,
				refresh: parsed.refresh_token ?? '',
				expires: parsed.expires_at ?? 0,
			};
		}
	} catch {
		return undefined;
	}
	return undefined;
}

function bearerOf(cred: Credential): string | undefined {
	if (cred.type === 'api_key') return cred.key;
	return cred.access;
}

registerProviderAuth({
	id: 'kimi-coding',
	displayName: 'Kimi Code (Kimi 会员)',
	async resolve({ authPath, env, cliAuthPaths }) {
		const existing = readCredential(authPath, 'kimi-coding');
		if (existing) return existing;

		const imported = readKimiCliCredentials(
			cliAuthPaths?.['kimi-coding'] ??
				join(homedir(), '.kimi-code', 'credentials', 'kimi-code.json'),
		);
		if (imported) {
			writeCredential(authPath, 'kimi-coding', imported);
			return imported;
		}

		const key = env.MOONSHOT_API_KEY ?? env.KIMI_API_KEY;
		if (key) {
			const cred: Credential = { type: 'api_key', key };
			writeCredential(authPath, 'kimi-coding', cred);
			return cred;
		}
		return undefined;
	},
	async smoke(cred) {
		const bearer = bearerOf(cred);
		if (!bearer) {
			return {
				ok: false,
				detail: '缺少凭证（kimi CLI OAuth 或 MOONSHOT_API_KEY）',
			};
		}
		try {
			const res = await httpFetch(KIMI_MODELS_URL, {
				headers: { authorization: `Bearer ${bearer}` },
				timeoutMs: 15_000,
			});
			const data = await readJson<{ data?: unknown[] }>(res);
			if (!res.ok) return { ok: false, detail: `HTTP ${res.status}` };
			return { ok: true, detail: `models: ${data.data?.length ?? 0}` };
		} catch (err) {
			return {
				ok: false,
				detail: err instanceof Error ? err.message : String(err),
			};
		}
	},
});
