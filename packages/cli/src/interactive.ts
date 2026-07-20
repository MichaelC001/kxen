import {
	createAgentSessionFromServices,
	createAgentSessionRuntime,
	createAgentSessionServices,
	InteractiveMode,
	ModelRegistry,
	runPrintMode,
	SessionManager,
} from '@earendil-works/pi-coding-agent';
import { ensureAgentDir } from '@kxen/core';
import { createKxenTools } from '@kxen/tools';

// TUI 复用 pi：InteractiveMode（全屏交互）与 runPrintMode（单发），不自造框架
interface RuntimeFlags {
	model?: string;
	plan?: boolean;
}

function readFlags(): RuntimeFlags {
	return {
		model: process.env.KXEN_MODEL || undefined,
		plan: process.env.KXEN_PLAN === '1',
	};
}

async function buildRuntime(cwd: string) {
	const agentDir = await ensureAgentDir();
	const flags = readFlags();
	const services = await createAgentSessionServices({ cwd, agentDir });
	const sessionManager = SessionManager.create(cwd);
	const planMode = flags.plan === true;
	const runtime = await createAgentSessionRuntime(
		async (opts) => {
			const created = await createAgentSessionFromServices({
				services,
				sessionManager: opts.sessionManager,
				customTools: createKxenTools({ execReadOnly: planMode }),
				...(planMode ? { tools: ['read', 'exec'] } : {}),
			});
			return { ...created, services, diagnostics: [] };
		},
		{ cwd, agentDir, sessionManager },
	);
	if (flags.model) {
		const registry = new ModelRegistry(runtime.session.modelRuntime);
		const [provider, ...rest] = flags.model.split('/');
		const modelId = rest.join('/');
		const model =
			(provider ? registry.find(provider, modelId) : undefined) ??
			registry.getAvailable().find((m) => m.id === modelId);
		if (model) await runtime.session.setModel(model);
	}
	return runtime;
}

export async function runInteractive(cwd: string): Promise<void> {
	const runtime = await buildRuntime(cwd);
	const mode = new InteractiveMode(runtime, {});
	await mode.run();
}

export async function runPrint(cwd: string, message: string): Promise<number> {
	const runtime = await buildRuntime(cwd);
	return runPrintMode(runtime, { mode: 'text', initialMessage: message });
}
