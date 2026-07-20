import { existsSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import type {
	ExtensionAPI,
	ExtensionCommandContext,
	InlineExtension,
} from '@earendil-works/pi-coding-agent';
import { EventBus, ensureAgentDir, KxenSession } from '@kxen/core';
import { GoalEngine, runGoal } from '@kxen/goal';
import { ModelResourceManager } from '@kxen/resources';
import { SubagentManager } from '@kxen/subagent';
import { createKxenTools } from '@kxen/tools';
import { WorkflowRuntime } from '@kxen/workflow';

interface GoalDraft {
	objective: string;
	completionCriteria: string;
	constraints?: string;
}

// /write-goal 的交互式起草流程（kimi write-goal 语义）：意图 -> 上下文 -> 判据 -> 确认
async function draftGoal(
	args: string,
	ctx: ExtensionCommandContext,
): Promise<GoalDraft | undefined> {
	let objective = args.trim();
	if (!objective) {
		objective =
			(await ctx.ui.input('目标是什么？（一句话描述要达成的事）'))?.trim() ??
			'';
	}
	if (!objective) {
		ctx.ui.notify('未提供目标，已取消', 'warning');
		return undefined;
	}

	const suggestedCriteria = `相关检查命令（如 bun run check 或项目测试）退出码为 0`;
	const completionCriteria =
		(
			await ctx.ui.input(
				`完成判据？（怎么算完成，可验证的）\n目标: ${objective.slice(0, 60)}`,
				suggestedCriteria,
			)
		)?.trim() ?? '';
	if (!completionCriteria) {
		ctx.ui.notify('完成判据是 goal 的必需项，已取消', 'warning');
		return undefined;
	}

	const constraints = (
		await ctx.ui.input('约束？（不能做什么，可留空）', '')
	)?.trim();

	const draft = [
		`目标: ${objective}`,
		`完成判据: ${completionCriteria}`,
		constraints ? `约束: ${constraints}` : null,
	]
		.filter(Boolean)
		.join('\n');
	const confirmed = await ctx.ui.confirm(
		'确认创建 goal',
		`${draft}\n\n确认后将创建并自动开始执行`,
	);
	if (!confirmed) {
		ctx.ui.notify('已取消', 'info');
		return undefined;
	}
	return {
		objective,
		completionCriteria,
		constraints: constraints || undefined,
	};
}

async function driveGoal(
	engine: GoalEngine,
	goalId: string,
	cwd: string,
	ctx: ExtensionCommandContext,
): Promise<void> {
	const goal = engine.get(goalId);
	if (!goal) return;
	ctx.ui.notify(
		`goal 启动: ${goal.contract.objective.slice(0, 60)}（后台执行）`,
		'info',
	);

	const bus = new EventBus();
	const session = await KxenSession.create({
		cwd,
		tools: createKxenTools(),
		bus,
	});

	const final = await runGoal(engine, goalId, {
		maxTurns: 8,
		verify: async () => {
			const proc = Bun.spawn(['bun', 'run', 'check'], {
				cwd,
				stdout: 'pipe',
				stderr: 'pipe',
			});
			const code = await proc.exited;
			return code === 0
				? { ok: true, evidence: 'bun run check 退出码为 0' }
				: { ok: false, evidence: `bun run check 退出码 ${code}` };
		},
		executeTurn: async (g, turn) => {
			ctx.ui.notify(`goal 第 ${turn + 1} 轮执行中...`, 'info');
			await session.prompt(
				`请完成这个任务：${g.contract.objective}\n约束：${g.contract.constraints ?? '无'}\n完成后不要解释，直接结束。`,
			);
			return { summary: 'agent 执行一轮' };
		},
	});
	await session.dispose();

	if (final.status === 'complete') {
		ctx.ui.notify(`goal 完成: ${final.verificationEvidence ?? ''}`, 'info');
	} else {
		ctx.ui.notify(
			`goal 结束于 ${final.status}${final.blockReason ? `: ${final.blockReason}` : ''}`,
			'warning',
		);
	}
}

function buildStack(cwd: string) {
	const bus = new EventBus();
	const mrm = new ModelResourceManager({
		roles: {
			execution: { model: 'xai/grok-4.5', fallbacks: ['kimi-coding/k3'] },
			review: {
				model: 'kimi-coding/k3',
				fallbacks: ['anthropic/claude-haiku-4-5'],
			},
			default: {
				model: 'anthropic/claude-fable-5',
				fallbacks: ['xai/grok-4.5'],
			},
		},
		limits: { global: { concurrent: 8 }, providers: {}, roles: {} },
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

export const kxenExtension: InlineExtension = {
	name: 'kxen-core',
	factory: (pi: ExtensionAPI) => {
		const engine = new GoalEngine({ bus: new EventBus() });

		pi.registerCommand('write-goal', {
			description: '交互式创建 goal 并自动开始执行',
			handler: async (args, ctx) => {
				const draft = await draftGoal(args, ctx);
				if (!draft) return;
				const goal = engine.create({
					objective: draft.objective,
					completionCriteria: draft.completionCriteria,
					constraints: draft.constraints,
				});
				await driveGoal(engine, goal.id, ctx.cwd, ctx);
			},
		});

		pi.registerCommand('goal', {
			description: '查看 goal 状态（/goal run 继续执行）',
			handler: async (args, ctx) => {
				const goals = engine.list();
				if (args.trim() === 'run') {
					const active = engine.activeGoal() ?? goals[0];
					if (!active) {
						ctx.ui.notify(
							'没有可执行的 goal，先用 /write-goal 创建',
							'warning',
						);
						return;
					}
					await driveGoal(engine, active.id, ctx.cwd, ctx);
					return;
				}
				if (goals.length === 0) {
					ctx.ui.notify('当前没有 goal，用 /write-goal 创建', 'info');
					return;
				}
				const lines = goals.map(
					(g) => `${g.id} [${g.status}] ${g.contract.objective.slice(0, 50)}`,
				);
				ctx.ui.notify(`goals:\n${lines.join('\n')}`, 'info');
			},
		});

		pi.registerCommand('workflow', {
			description: '运行 workflow 脚本（/workflow <脚本路径.js>）',
			handler: async (args, ctx) => {
				const path = args.trim();
				if (!path || !existsSync(path)) {
					ctx.ui.notify(
						'用法: /workflow <脚本路径.js>（脚本用 agent()/pipeline() 编排）',
						'warning',
					);
					return;
				}
				const script = await readFile(path, 'utf8');
				const { manager } = buildStack(ctx.cwd);
				const runtime = new WorkflowRuntime({
					bus: new EventBus(),
					executeAgent: async (prompt, _spec, opts) => {
						const handle = await manager.spawn(
							{
								name: 'execute',
								description: 'workflow agent',
								role: opts.role ?? 'execution',
							},
							prompt,
						);
						return handle.result;
					},
					constraintsProvider: () => ({}),
				});
				ctx.ui.notify(`workflow 启动: ${path}`, 'info');
				const state = await runtime.run(script);
				if (state.status === 'completed') {
					const result =
						typeof state.result === 'string'
							? state.result
							: JSON.stringify(state.result);
					ctx.ui.notify(
						`workflow 完成（${state.agentCalls} 次调用）:\n${result?.slice(0, 800)}`,
						'info',
					);
				} else {
					ctx.ui.notify(
						`workflow ${state.status}: ${state.error ?? ''}`,
						'error',
					);
				}
			},
		});
	},
};
