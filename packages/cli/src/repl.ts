import * as readline from 'node:readline/promises';
import { ModelRegistry } from '@earendil-works/pi-coding-agent';
import {
	EventBus,
	ensureAgentDir,
	KXEN_AUTH_PATH,
	KxenSession,
	loadConfig,
	MODES,
	type Mode,
} from '@kxen/core';
import { getProviderAuth } from '@kxen/providers';
import { ModelResourceManager } from '@kxen/resources';
import { createKxenTools } from '@kxen/tools';
import { renderStatusline } from '@kxen/tui';

export interface ReplOptions {
	cwd: string;
	oneShot?: string;
}

async function createSession(
	cwd: string,
	mode: Mode,
	bus: EventBus,
): Promise<KxenSession> {
	const spec = MODES[mode];
	return KxenSession.create({
		cwd,
		tools: createKxenTools({ execReadOnly: spec.execReadOnly }),
		allowedTools: spec.allowedTools,
		bus,
	});
}

async function ensureAnthropicAuth(): Promise<void> {
	const auth = getProviderAuth('anthropic');
	if (!auth) throw new Error('anthropic provider 未注册');
	const cred = await auth.resolve({
		authPath: KXEN_AUTH_PATH,
		env: process.env,
	});
	if (!cred)
		throw new Error(
			'未找到 Claude 凭证：请先在本机登录 Claude Code，或设置 ANTHROPIC_API_KEY',
		);
}

async function selectModel(session: KxenSession): Promise<void> {
	const registry = new ModelRegistry(session.inner.modelRuntime);
	const available = registry.getAvailable();
	const claude = available.find((m) => m.provider === 'anthropic');
	const model = claude ?? available[0];
	if (model) await session.inner.setModel(model);
}

function printLastAssistantText(session: KxenSession): void {
	const messages = session.inner.agent.state.messages as Array<{
		role?: string;
		content?: Array<{ type?: string; text?: string }> | string;
	}>;
	const last = [...messages].reverse().find((m) => m.role === 'assistant');
	if (!last) return;
	const text =
		typeof last.content === 'string'
			? last.content
			: (last.content ?? [])
					.filter((c) => c.type === 'text')
					.map((c) => c.text)
					.join('\n');
	if (text) console.log(`\n${text}\n`);
}

export async function runRepl(opts: ReplOptions): Promise<void> {
	ensureAgentDir();
	await ensureAnthropicAuth();
	const bus = new EventBus();
	const cfg = loadConfig({ projectDir: opts.cwd });
	const mrm = new ModelResourceManager({
		roles:
			Object.keys(cfg.roles).length > 0
				? cfg.roles
				: { default: { model: 'anthropic/claude-fable-5' } },
		limits: cfg.limits,
		bus,
	});

	let mode: Mode = 'build';
	let session = await createSession(opts.cwd, mode, bus);
	await selectModel(session);

	const modelName = () => {
		const m = session.inner.model;
		return m ? `${m.provider}/${m.id}` : 'no-model';
	};
	const showStatus = () => {
		console.log(
			renderStatusline({
				model: modelName(),
				mode,
				mrm: mrm.status(),
				cwd: opts.cwd,
			}),
		);
	};

	if (opts.oneShot !== undefined) {
		const slot = await mrm.acquire({ role: 'default', priority: 5 });
		try {
			await session.prompt(opts.oneShot);
			printLastAssistantText(session);
		} finally {
			mrm.release(slot, { ok: true });
			await session.dispose();
		}
		return;
	}

	const rl = readline.createInterface({
		input: process.stdin,
		output: process.stdout,
	});
	console.log(
		`kxen 交互模式（/mode 切换，/model 模型，/status 状态，/quit 退出）`,
	);
	showStatus();
	for (;;) {
		const line = (await rl.question(`kxen:${mode}> `)).trim();
		if (!line) continue;
		if (line === '/quit' || line === '/exit') break;
		if (line === '/mode') {
			mode = mode === 'build' ? 'plan' : 'build';
			await session.dispose();
			session = await createSession(opts.cwd, mode, bus);
			await selectModel(session);
			console.log(`已切换为 ${mode} 模式`);
			continue;
		}
		if (line === '/model') {
			console.log(`当前模型: ${modelName()}`);
			continue;
		}
		if (line === '/status') {
			showStatus();
			continue;
		}
		const slot = await mrm.acquire({ role: 'default', priority: 5 });
		try {
			await session.prompt(line);
			printLastAssistantText(session);
		} catch (err) {
			mrm.release(slot, { ok: false, error: err });
			console.error(
				`错误: ${err instanceof Error ? err.message : String(err)}`,
			);
			continue;
		}
		mrm.release(slot, { ok: true });
	}
	rl.close();
	await session.dispose();
}
