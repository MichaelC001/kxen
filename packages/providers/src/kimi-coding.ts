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
async function readKimiCliCredentials(
	credPath: string,
): Promise<Credential | undefined> {
	const file = Bun.file(credPath);
	if (!(await file.exists())) return undefined;
	try {
		const parsed = (await file.json()) as KimiCliCredentials;
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
		// kimi CLI 会轮换 OAuth token，存储副本极易过期：优先用 CLI 文件里的新鲜凭证
		const imported = await readKimiCliCredentials(
			cliAuthPaths?.['kimi-coding'] ??
				join(homedir(), '.kimi-code', 'credentials', 'kimi-code.json'),
		);
		if (imported) {
			// pi 对 OpenAI 兼容 provider 只认 api_key 型凭证；kimi access token 可直接作 Bearer key
			const asKey: Credential =
				imported.type === 'oauth'
					? { type: 'api_key', key: imported.access }
					: imported;
			const existing = await readCredential(authPath, 'kimi-coding');
			if (
				existing?.type === 'api_key' &&
				existing.key === (asKey as { key?: string }).key
			) {
				return existing;
			}
			await writeCredential(authPath, 'kimi-coding', asKey);
			return asKey;
		}

		const existing = await readCredential(authPath, 'kimi-coding');
		if (existing) return existing;

		const key = env.MOONSHOT_API_KEY ?? env.KIMI_API_KEY;
		if (key) {
			const cred: Credential = { type: 'api_key', key };
			await writeCredential(authPath, 'kimi-coding', cred);
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
