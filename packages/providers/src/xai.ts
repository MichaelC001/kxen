import { existsSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { type Credential, readCredential, writeCredential } from './auth-file';
import { registerProviderAuth } from './registry';

// grok-build 会话 token 存放于 ~/.grok/auth.json
// 结构为 issuer 键控 map: { "https://auth.x.ai::<client_id>": { key, refresh_token, expires_at, ... } }
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

export const XAI_OAUTH_ENDPOINTS = {
	authorize: 'https://auth.x.ai/oauth2/authorize',
	token: 'https://auth.x.ai/oauth2/token',
	deviceCode: 'https://auth.x.ai/oauth2/device/code',
	// xAI 公开的 grok-cli desktop OAuth client（OpenCode 同样复用，见 research/03）
	clientId: 'b1a00492-073a-47ea-816f-4c329264a828',
	scope: 'openid profile email offline_access grok-cli:access api:access',
} as const;

export interface DeviceCodeResponse {
	device_code: string;
	user_code: string;
	verification_uri: string;
	verification_uri_complete?: string;
	expires_in: number;
	interval?: number;
}

// RFC 8628 device code 流程（headless 场景）；fetch 可注入便于测试
export async function requestDeviceCode(
	fetchImpl: typeof fetch = fetch,
): Promise<DeviceCodeResponse> {
	const res = await fetchImpl(XAI_OAUTH_ENDPOINTS.deviceCode, {
		method: 'POST',
		headers: { 'content-type': 'application/x-www-form-urlencoded' },
		body: new URLSearchParams({
			client_id: XAI_OAUTH_ENDPOINTS.clientId,
			scope: XAI_OAUTH_ENDPOINTS.scope,
		}).toString(),
	});
	if (!res.ok) throw new Error(`xAI device code 请求失败 (${res.status})`);
	return (await res.json()) as DeviceCodeResponse;
}

export async function pollDeviceToken(
	deviceCode: string,
	fetchImpl: typeof fetch = fetch,
): Promise<Credential> {
	const res = await fetchImpl(XAI_OAUTH_ENDPOINTS.token, {
		method: 'POST',
		headers: { 'content-type': 'application/x-www-form-urlencoded' },
		body: new URLSearchParams({
			client_id: XAI_OAUTH_ENDPOINTS.clientId,
			grant_type: 'urn:ietf:params:oauth:grant-type:device_code',
			device_code: deviceCode,
		}).toString(),
	});
	if (!res.ok) throw new Error(`xAI token 轮询失败 (${res.status})`);
	const data = (await res.json()) as {
		access_token: string;
		refresh_token?: string;
		expires_in?: number;
	};
	return {
		type: 'oauth',
		access: data.access_token,
		refresh: data.refresh_token ?? '',
		expires: Date.now() + (data.expires_in ?? 3600) * 1000,
	};
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
