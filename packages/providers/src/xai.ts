import { homedir } from 'node:os';
import { join } from 'node:path';
import { type Credential, readCredential, writeCredential } from './auth-file';
import { registerProviderAuth } from './registry';

// 只负责把官方 CLI 的现有凭证搬进 pi 的 auth.json；OAuth 流程由 pi-ai 内置的 xai 处理
async function readGrokCliCredentials(
	credPath: string,
): Promise<Credential | undefined> {
	const file = Bun.file(credPath);
	if (!(await file.exists())) return undefined;
	try {
		const parsed = (await file.json()) as Record<
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
		// grok CLI 会轮换 token：优先用官方文件里的新鲜凭证
		const imported = await readGrokCliCredentials(
			cliAuthPaths?.xai ?? join(homedir(), '.grok', 'auth.json'),
		);
		if (imported) {
			const existing = await readCredential(authPath, 'xai');
			const fresh =
				!existing ||
				existing.type !== 'oauth' ||
				(imported.type === 'oauth' &&
					(imported.expires ?? 0) > (existing.expires ?? 0));
			if (fresh) {
				await writeCredential(authPath, 'xai', imported);
				return imported;
			}
			return existing;
		}

		const existing = await readCredential(authPath, 'xai');
		if (existing) return existing;

		if (env.XAI_API_KEY) {
			const cred: Credential = { type: 'api_key', key: env.XAI_API_KEY };
			await writeCredential(authPath, 'xai', cred);
			return cred;
		}
		return undefined;
	},
});
