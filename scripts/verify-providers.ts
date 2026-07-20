// 四订阅真实调用验证：每个 provider 一次真实 API 调用
// 用法: bun run scripts/verify-providers.ts

import { existsSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { httpFetch, readJson } from '../packages/core/src/http';
import { ensureAgentDir, KXEN_AUTH_PATH } from '../packages/core/src/provider';
import {
	type Credential,
	readCredential,
} from '../packages/providers/src/auth-file';
import {
	getProviderAuth,
	listProviderAuths,
} from '../packages/providers/src/index';

function bearerOf(cred: Credential): string | undefined {
	if (cred.type === 'api_key') return cred.key;
	return cred.access;
}

async function checkAnthropic(
	cred: Credential,
): Promise<{ ok: boolean; detail: string }> {
	const bearer = bearerOf(cred);
	if (!bearer) return { ok: false, detail: '无凭证' };
	const res = await httpFetch('https://api.anthropic.com/v1/models', {
		headers: {
			authorization: `Bearer ${bearer}`,
			'anthropic-version': '2023-06-01',
			'anthropic-beta': 'oauth-2025-04-20',
		},
		timeoutMs: 15_000,
	});
	await readJson(res);
	if (res.ok) return { ok: true, detail: 'HTTP 200' };
	return { ok: false, detail: `HTTP ${res.status}` };
}

async function checkCodex(
	cred: Credential,
): Promise<{ ok: boolean; detail: string }> {
	const bearer = bearerOf(cred);
	if (!bearer) return { ok: false, detail: '无凭证' };
	const accountId = (cred as Record<string, unknown>).account_id as
		| string
		| undefined;
	// codex backend 的可用模型随账户 entitlement 变化，直接读 codex CLI 当前配置的模型名
	let model = 'gpt-5.1-codex';
	const configPath = join(homedir(), '.codex', 'config.toml');
	if (existsSync(configPath)) {
		const cfg = Bun.TOML.parse(readFileSync(configPath, 'utf8')) as {
			model?: string;
		};
		if (cfg.model) model = cfg.model;
	}
	const res = await httpFetch(
		'https://chatgpt.com/backend-api/codex/responses',
		{
			method: 'POST',
			headers: {
				authorization: `Bearer ${bearer}`,
				'content-type': 'application/json',
				'OpenAI-Beta': 'responses=experimental',
				originator: 'codex_cli_rs',
				...(accountId ? { 'chatgpt-account-id': accountId } : {}),
			},
			body: JSON.stringify({
				model,
				instructions: 'You are a helpful assistant.',
				input: [
					{
						type: 'message',
						role: 'user',
						content: [{ type: 'input_text', text: 'say ok' }],
					},
				],
				stream: true,
				store: false,
			}),
			timeoutMs: 30_000,
		},
	);
	if (res.ok) return { ok: true, detail: `HTTP 200 (model=${model})` };
	if (res.status === 401 || res.status === 403)
		return { ok: false, detail: `HTTP ${res.status}` };
	return { ok: true, detail: `HTTP ${res.status}（非 401/403，凭证被接受）` };
}

async function checkXai(
	cred: Credential,
): Promise<{ ok: boolean; detail: string }> {
	const bearer = bearerOf(cred);
	if (!bearer) return { ok: false, detail: '无凭证' };
	const res = await httpFetch('https://api.x.ai/v1/models', {
		headers: { authorization: `Bearer ${bearer}` },
		timeoutMs: 15_000,
	});
	await readJson(res);
	if (res.ok) return { ok: true, detail: 'HTTP 200' };
	return { ok: false, detail: `HTTP ${res.status}` };
}

async function checkKimi(
	cred: Credential,
): Promise<{ ok: boolean; detail: string }> {
	const auth = getProviderAuth('kimi-coding');
	if (!auth?.smoke) return { ok: false, detail: '无 smoke 实现' };
	return auth.smoke(cred);
}

const checkers: Record<
	string,
	(cred: Credential) => Promise<{ ok: boolean; detail: string }>
> = {
	anthropic: checkAnthropic,
	'openai-codex': checkCodex,
	xai: checkXai,
	'kimi-coding': checkKimi,
};

ensureAgentDir();
const results: { id: string; status: string; detail: string }[] = [];

for (const auth of listProviderAuths()) {
	const cred = await auth.resolve({
		authPath: KXEN_AUTH_PATH,
		env: process.env,
	});
	if (!cred) {
		results.push({
			id: auth.id,
			status: 'blocked',
			detail: '本机无凭证（需官方 CLI 已登录或环境变量）',
		});
		continue;
	}
	const checker = checkers[auth.id];
	if (!checker) {
		results.push({ id: auth.id, status: 'skip', detail: '无 checker' });
		continue;
	}
	try {
		const r = await checker(cred);
		results.push({
			id: auth.id,
			status: r.ok ? 'PASS' : 'FAIL',
			detail: r.detail,
		});
	} catch (err) {
		results.push({
			id: auth.id,
			status: 'FAIL',
			detail: err instanceof Error ? err.message : String(err),
		});
	}
}

console.log('\n四订阅验证结果:');
for (const r of results) {
	console.log(`  ${r.status.padEnd(7)} ${r.id.padEnd(14)} ${r.detail}`);
}

const anyFail = results.some((r) => r.status === 'FAIL');
process.exit(anyFail ? 1 : 0);
