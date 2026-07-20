// E2E 共享编排栈：真实会话 + MRM + SubagentManager
import {
	EventBus,
	ensureAgentDir,
	KXEN_AUTH_PATH,
	KxenSession,
} from '@kxen/core';
import { listProviderAuths } from '@kxen/providers';
import { ModelResourceManager } from '@kxen/resources';
import { SubagentManager } from '@kxen/subagent';
import { createKxenTools } from '@kxen/tools';

export interface Stack {
	bus: EventBus;
	mrm: ModelResourceManager;
	manager: SubagentManager;
}

// 启动时统一解析全部已注册 provider 的凭证（导入官方 CLI 现有凭证）
export async function resolveAllProviders(): Promise<Record<string, boolean>> {
	const out: Record<string, boolean> = {};
	for (const auth of listProviderAuths()) {
		const cred = await auth.resolve({
			authPath: KXEN_AUTH_PATH,
			env: process.env,
		});
		out[auth.id] = !!cred;
	}
	return out;
}

export async function buildStack(cwd: string): Promise<Stack> {
	ensureAgentDir();
	await resolveAllProviders();
	const bus = new EventBus();
	const mrm = new ModelResourceManager({
		roles: {
			execution: {
				model: 'xai/grok-4.5',
				fallbacks: ['kimi-coding/k3', 'anthropic/claude-haiku-4-5'],
			},
			review: {
				model: 'kimi-coding/k3',
				fallbacks: ['openai-codex/gpt-5.4-mini', 'anthropic/claude-haiku-4-5'],
			},
			default: {
				model: 'anthropic/claude-fable-5',
				fallbacks: ['xai/grok-4.5'],
			},
			tiny: {
				model: 'openai-codex/gpt-5.4-mini',
				fallbacks: ['kimi-coding/kimi-for-coding'],
			},
		},
		limits: {
			global: { concurrent: 8 },
			providers: {
				anthropic: { concurrent: 4 },
				xai: { concurrent: 4 },
				'kimi-coding': { concurrent: 3 },
				'openai-codex': { concurrent: 3 },
			},
			roles: {},
		},
		bus,
	});
	const manager = new SubagentManager({
		mrm,
		bus,
		createSession: async (opts) =>
			KxenSession.create({
				cwd: opts.cwd,
				tools: createKxenTools(),
				allowedTools: opts.allowedTools,
				bus: opts.bus,
			}),
		toolsFor: () => createKxenTools(),
		repoRoot: cwd,
	});
	return { bus, mrm, manager };
}
