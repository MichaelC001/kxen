import { existsSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { type Credential, readCredential, writeCredential } from './auth-file';
import { registerProviderAuth } from './registry';

interface CodexAuthFile {
	OPENAI_API_KEY?: string;
	tokens?: {
		access_token?: string;
		refresh_token?: string;
		id_token?: string;
		account_id?: string;
	};
}

function decodeJwtExp(token: string): number {
	try {
		const payload = token.split('.')[1] ?? '';
		const claims = JSON.parse(Buffer.from(payload, 'base64url').toString()) as {
			exp?: number;
		};
		return (claims.exp ?? 0) * 1000;
	} catch {
		return 0;
	}
}

// Codex CLI 的 ChatGPT OAuth 凭证（~/.codex/auth.json，CLI 与 IDE 共享，自动刷新）
function readCodexCliCredentials(credPath: string): Credential | undefined {
	if (!existsSync(credPath)) return undefined;
	try {
		const parsed = JSON.parse(readFileSync(credPath, 'utf8')) as CodexAuthFile;
		if (parsed.tokens?.access_token) {
			return {
				type: 'oauth',
				access: parsed.tokens.access_token,
				refresh: parsed.tokens.refresh_token ?? '',
				expires: decodeJwtExp(parsed.tokens.access_token),
				account_id: parsed.tokens.account_id,
			};
		}
		if (parsed.OPENAI_API_KEY) {
			return { type: 'api_key', key: parsed.OPENAI_API_KEY };
		}
	} catch {
		return undefined;
	}
	return undefined;
}

export const CODEX_OAUTH = {
	tokenUrl: 'https://auth.openai.com/oauth/token',
	// codex CLI 的公开 OAuth client id（codex-rs 内置）
	clientId: 'app_EMoamEEZ73f0CkXaXp7hrann',
} as const;

// 刷新 ChatGPT OAuth token；fetch 可注入便于测试
export async function refreshCodexToken(
	refreshToken: string,
	fetchImpl: typeof fetch = fetch,
): Promise<Credential> {
	const res = await fetchImpl(CODEX_OAUTH.tokenUrl, {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({
			client_id: CODEX_OAUTH.clientId,
			grant_type: 'refresh_token',
			refresh_token: refreshToken,
		}),
	});
	if (!res.ok) throw new Error(`codex token 刷新失败 (${res.status})`);
	const data = (await res.json()) as {
		access_token: string;
		refresh_token?: string;
		id_token?: string;
	};
	return {
		type: 'oauth',
		access: data.access_token,
		refresh: data.refresh_token ?? refreshToken,
		expires: decodeJwtExp(data.access_token),
	};
}

registerProviderAuth({
	id: 'openai-codex',
	displayName: 'Codex (ChatGPT Plus/Pro)',
	async resolve({ authPath, env, cliAuthPaths }) {
		const existing = readCredential(authPath, 'openai-codex');
		if (existing) {
			const effectiveExpires =
				existing.type === 'oauth'
					? existing.expires > 0
						? existing.expires
						: decodeJwtExp(existing.access)
					: 0;
			if (
				existing.type === 'oauth' &&
				effectiveExpires > 0 &&
				effectiveExpires < Date.now() &&
				existing.refresh
			) {
				try {
					const refreshed = await refreshCodexToken(existing.refresh);
					const merged = {
						...refreshed,
						account_id: existing.account_id,
					} as Credential;
					writeCredential(authPath, 'openai-codex', merged);
					return merged;
				} catch {
					// 继续走重新导入 / env
				}
			} else {
				return existing;
			}
		}

		const imported = readCodexCliCredentials(
			cliAuthPaths?.['openai-codex'] ?? join(homedir(), '.codex', 'auth.json'),
		);
		if (imported) {
			if (
				imported.type === 'oauth' &&
				imported.expires > 0 &&
				imported.expires < Date.now() &&
				imported.refresh
			) {
				const refreshed = await refreshCodexToken(imported.refresh);
				const merged = {
					...refreshed,
					account_id: imported.account_id,
				} as Credential;
				writeCredential(authPath, 'openai-codex', merged);
				return merged;
			}
			writeCredential(authPath, 'openai-codex', imported);
			return imported;
		}

		if (env.OPENAI_API_KEY) {
			const cred: Credential = { type: 'api_key', key: env.OPENAI_API_KEY };
			writeCredential(authPath, 'openai-codex', cred);
			return cred;
		}
		return undefined;
	},
});
