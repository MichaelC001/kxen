import { existsSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { type Credential, readCredential, writeCredential } from './auth-file';
import { registerProviderAuth } from './registry';

// 只负责把官方 CLI 的现有凭证搬进 pi 的 auth.json；OAuth 流程由 pi-ai 内置的 xai 处理
function readGrokCliCredentials(credPath: string): Credential | undefined {
	if (!existsSync(credPath)) return undefined;
	try {
		const parsed = JSON.parse(readFileSync(credPath, 'utf8')) as Record<
			string,
			Record<string, unknown>
		>;
		for (const entry of Object.values(parsed)) {
			const access = entry.key as string | undefined;
			if (!access) continue;
			const expiresAt = entry.expires_at as string | undefined;
			return {
				type: 'oauth',
				access,
				refresh: (entry.refresh_token as string | undefined) ?? '',
				expires: expiresAt ? Date.parse(expiresAt) : 0,
			};
		}
	} catch {
		return undefined;
	}
	return undefined;
}

registerProviderAuth({
	id: 'xai',
	displayName: 'Grok (SuperGrok / X Premium+)',
	async resolve({ authPath, env, cliAuthPaths }) {
		const existing = readCredential(authPath, 'xai');
		if (existing) return existing;

		const imported = readGrokCliCredentials(
			cliAuthPaths?.xai ?? join(homedir(), '.grok', 'auth.json'),
		);
		if (imported) {
			writeCredential(authPath, 'xai', imported);
			return imported;
		}

		if (env.XAI_API_KEY) {
			const cred: Credential = { type: 'api_key', key: env.XAI_API_KEY };
			writeCredential(authPath, 'xai', cred);
			return cred;
		}
		return undefined;
	},
});
