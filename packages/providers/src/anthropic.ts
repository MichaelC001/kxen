import { existsSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { type Credential, readCredential, writeCredential } from './auth-file';
import { registerProviderAuth } from './registry';

interface ClaudeCodeCredentialsFile {
	claudeAiOauth?: {
		accessToken: string;
		refreshToken: string;
		expiresAt: number;
	};
}

// macOS: Claude Code 凭证在 Keychain；其他平台: ~/.claude/.credentials.json
async function readClaudeCodeCredentials(): Promise<Credential | undefined> {
	const credPath = join(homedir(), '.claude', '.credentials.json');
	let raw: string | undefined;
	if (existsSync(credPath)) {
		raw = readFileSync(credPath, 'utf8');
	} else if (process.platform === 'darwin') {
		const proc = Bun.spawn(
			[
				'security',
				'find-generic-password',
				'-s',
				'Claude Code-credentials',
				'-w',
			],
			{
				stdout: 'pipe',
				stderr: 'pipe',
			},
		);
		const out = await new Response(proc.stdout).text();
		if ((await proc.exited) === 0 && out.trim()) raw = out.trim();
	}
	if (!raw) return undefined;
	try {
		const parsed = JSON.parse(raw) as ClaudeCodeCredentialsFile;
		const oauth = parsed.claudeAiOauth;
		if (!oauth) return undefined;
		return {
			type: 'oauth',
			access: oauth.accessToken,
			refresh: oauth.refreshToken,
			expires: oauth.expiresAt,
		};
	} catch {
		return undefined;
	}
}

registerProviderAuth({
	id: 'anthropic',
	displayName: 'Claude (Anthropic Pro/Max)',
	async resolve({ authPath, env }) {
		// Claude Code 会自动轮换 OAuth token，存储副本极易过期：优先用官方存储里的新鲜凭证
		const imported = await readClaudeCodeCredentials();
		if (imported) {
			const existing = readCredential(authPath, 'anthropic');
			const fresh =
				!existing ||
				existing.type !== 'oauth' ||
				(imported.type === 'oauth' &&
					(imported.expires ?? 0) > (existing.expires ?? 0));
			if (fresh) {
				writeCredential(authPath, 'anthropic', imported);
				return imported;
			}
			return existing;
		}

		const existing = readCredential(authPath, 'anthropic');
		if (existing) return existing;

		if (env.ANTHROPIC_API_KEY) {
			const cred: Credential = { type: 'api_key', key: env.ANTHROPIC_API_KEY };
			writeCredential(authPath, 'anthropic', cred);
			return cred;
		}
		return undefined;
	},
});
