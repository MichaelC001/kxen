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

// 只负责把官方 CLI 的现有凭证搬进 pi 的 auth.json；OAuth 刷新与请求由 pi-ai 内置的 openai-codex 处理
async function readCodexCliCredentials(
	credPath: string,
): Promise<Credential | undefined> {
	const file = Bun.file(credPath);
	if (!(await file.exists())) return undefined;
	try {
		const parsed = (await file.json()) as CodexAuthFile;
		if (parsed.tokens?.access_token) {
			return {
				type: 'oauth',
				access: parsed.tokens.access_token,
				refresh: parsed.tokens.refresh_token ?? '',
				expires: 0,
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

registerProviderAuth({
	id: 'openai-codex',
	displayName: 'Codex (ChatGPT Plus/Pro)',
	async resolve({ authPath, env, cliAuthPaths }) {
		const existing = await readCredential(authPath, 'openai-codex');
		if (existing) return existing;

		const imported = await readCodexCliCredentials(
			cliAuthPaths?.['openai-codex'] ?? join(homedir(), '.codex', 'auth.json'),
		);
		if (imported) {
			await writeCredential(authPath, 'openai-codex', imported);
			return imported;
		}

		if (env.OPENAI_API_KEY) {
			const cred: Credential = { type: 'api_key', key: env.OPENAI_API_KEY };
			await writeCredential(authPath, 'openai-codex', cred);
			return cred;
		}
		return undefined;
	},
});
