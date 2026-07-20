import type { EventBus } from '@kxen/core';

// 边界约定：shell command hooks 与 HTTP hooks 是 kxen 特有（pi 无此能力，对齐 Claude Code 语义）；
// builtin TS hook 仅限 kxen 自身内置逻辑，用户级 TS 扩展一律走 pi 的 extension 体系（不重复造）;

export const HOOK_EVENTS = [
	'SessionStart',
	'SessionEnd',
	'UserPromptSubmit',
	'PreToolUse',
	'PostToolUse',
	'PostToolUseFailure',
	'PostToolBatch',
	'SubagentSpawn',
	'SubagentComplete',
	'GoalCreated',
	'GoalCompleted',
	'WorkflowStart',
	'WorkflowEnd',
	'MRMDegraded',
	'BudgetWarning',
	'PreCompact',
	'Stop',
] as const;

export type HookEvent = (typeof HOOK_EVENTS)[number];

export type HookDecision = 'allow' | 'deny' | 'ask' | 'defer';

export interface HookInput {
	event: HookEvent;
	toolName?: string;
	toolInput?: Record<string, unknown>;
	payload?: Record<string, unknown>;
	cwd: string;
}

export interface HookOutput {
	decision?: HookDecision;
	reason?: string;
	additionalContext?: string;
	updatedInput?: Record<string, unknown>;
}

export type BuiltinHookHandler = (
	input: HookInput,
) => Promise<HookOutput | undefined> | HookOutput | undefined;

export interface CommandHook {
	kind: 'command';
	command: string;
	timeoutMs?: number;
}

export interface HttpHook {
	kind: 'http';
	url: string;
	timeoutMs?: number;
}

export interface BuiltinHook {
	kind: 'builtin';
	handler: BuiltinHookHandler;
}

export interface RegisteredHook {
	id: string;
	event: HookEvent;
	matcher?: string;
	hook: CommandHook | HttpHook | BuiltinHook;
	enabled: boolean;
	source: 'builtin' | 'global' | 'project';
}

// 优先级：deny > defer > ask > allow（analysis/08）
export function pickDecision(
	decisions: HookDecision[],
): HookDecision | undefined {
	for (const d of ['deny', 'defer', 'ask', 'allow'] as const) {
		if (decisions.includes(d)) return d;
	}
	return undefined;
}

function matchPattern(
	matcher: string | undefined,
	toolName: string | undefined,
): boolean {
	if (!matcher) return true;
	if (!toolName) return false;
	for (const alt of matcher.split('|')) {
		const trimmed = alt.trim();
		if (!trimmed) continue;
		if (trimmed === toolName) return true;
		if (
			trimmed.includes('*') &&
			new RegExp(`^${trimmed.replace(/\*/g, '.*')}$`).test(toolName)
		)
			return true;
	}
	return false;
}

export class HookRegistry {
	private hooks: RegisteredHook[] = [];

	constructor(private bus: EventBus) {}

	register(hook: RegisteredHook): void {
		this.hooks.push(hook);
	}

	setEnabled(id: string, enabled: boolean): void {
		const hook = this.hooks.find((h) => h.id === id);
		if (hook) hook.enabled = enabled;
	}

	list(): readonly RegisteredHook[] {
		return this.hooks;
	}

	async run(
		event: HookEvent,
		input: Omit<HookInput, 'event'>,
	): Promise<HookOutput[]> {
		const outputs: HookOutput[] = [];
		for (const hook of this.hooks) {
			if (!hook.enabled || hook.event !== event) continue;
			if (!matchPattern(hook.matcher, input.toolName)) continue;
			try {
				const out = await this.execute(hook, { ...input, event });
				if (out) outputs.push(out);
				this.bus.publish('hook.fired', {
					id: hook.id,
					event,
					decision: out?.decision,
				});
			} catch (err) {
				this.bus.publish('hook.error', {
					id: hook.id,
					event,
					error: err instanceof Error ? err.message : String(err),
				});
			}
		}
		return outputs;
	}

	private async execute(
		hook: RegisteredHook,
		input: HookInput,
	): Promise<HookOutput | undefined> {
		if (hook.hook.kind === 'builtin') {
			return hook.hook.handler(input);
		}
		if (hook.hook.kind === 'command') {
			return this.executeCommand(hook.hook, input);
		}
		return this.executeHttp(hook.hook, input);
	}

	private async executeCommand(
		hook: CommandHook,
		input: HookInput,
	): Promise<HookOutput | undefined> {
		const proc = Bun.spawn(['/bin/zsh', '-c', hook.command], {
			stdin: 'pipe',
			stdout: 'pipe',
			stderr: 'pipe',
		});
		proc.stdin.write(JSON.stringify(input));
		proc.stdin.end();
		const [stdout, stderr, code] = await Promise.all([
			new Response(proc.stdout).text(),
			new Response(proc.stderr).text(),
			proc.exited,
		]);
		// exit 2 = 阻断，stderr 反馈给模型（CC 语义）
		if (code === 2) return { decision: 'deny', reason: stderr.trim() };
		if (code !== 0) return undefined;
		const trimmed = stdout.trim();
		if (!trimmed) return undefined;
		try {
			return JSON.parse(trimmed) as HookOutput;
		} catch {
			return { additionalContext: trimmed };
		}
	}

	private async executeHttp(
		hook: HttpHook,
		input: HookInput,
	): Promise<HookOutput | undefined> {
		try {
			const res = await fetch(hook.url, {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify(input),
				signal: AbortSignal.timeout(hook.timeoutMs ?? 10_000),
			});
			// HTTP hooks 非 2xx 一律非阻断（hook 服务挂掉不能卡死会话）
			if (!res.ok) return undefined;
			return (await res.json()) as HookOutput;
		} catch {
			return undefined;
		}
	}
}
