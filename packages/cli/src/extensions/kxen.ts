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
import { Type } from 'typebox';

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
			thinking: {
				model: 'anthropic/claude-fable-5',
				fallbacks: ['openai-codex/gpt-5.5'],
			},
			planning: {
				model: 'anthropic/claude-fable-5',
				fallbacks: ['kimi-coding/k3'],
			},
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

		// 模型自主调用的工具：子代理与 workflow 是模型行为，不是用户命令
		pi.registerTool({
			name: 'subagent',
			label: 'Subagent',
			description:
				'派发子代理执行独立子任务，返回 typed 结果。适合：并行探索多个方向、把互不冲突的子任务 fan-out 出去保持主上下文干净。单个任务传 task；批量并行传 items。',
			promptGuidelines: [
				'多个互不冲突的子任务时用 items 一次 fan-out，不要逐个串行调用',
				'子代理只拿最终结果，过程不进主上下文',
			],
			parameters: Type.Object({
				task: Type.Optional(Type.String({ description: '单个子任务描述' })),
				items: Type.Optional(
					Type.Array(Type.String(), { description: '批量子任务（并行）' }),
				),
				role: Type.Optional(
					Type.Union(
						[
							Type.Literal('execution'),
							Type.Literal('review'),
							Type.Literal('research'),
							Type.Literal('planning'),
						],
						{
							description: '子代理角色（模型路由用），默认 execution',
						},
					),
				),
			}),
			async execute(_id, params, _signal, _onUpdate, ctx) {
				const { manager } = buildStack(ctx.cwd);
				const role = params.role ?? 'execution';
				const spec = { name: role, description: `${role} subagent`, role };
				if (params.items && params.items.length > 0) {
					const handles = await manager.swarm(spec, params.items);
					const results = await Promise.all(handles.map((h) => h.result));
					const text = results
						.map((r, i) => `[${i + 1}] ${r.summary}`)
						.join('\n\n');
					return {
						content: [{ type: 'text' as const, text }],
						details: undefined,
					};
				}
				if (!params.task) {
					return {
						content: [
							{ type: 'text' as const, text: '需要 task 或 items 参数' },
						],
						details: undefined,
					};
				}
				const handle = await manager.spawn(spec, params.task);
				const result = await handle.result;
				return {
					content: [{ type: 'text' as const, text: result.summary }],
					details: undefined,
				};
			},
		});

		pi.registerTool({
			name: 'workflow_run',
			label: 'Workflow Run',
			description:
				'执行你编写的 workflow 编排脚本。你（模型）根据任务自己写脚本：agent()/pipeline()/constraints()/phase() 可用，顶层 return 汇总结果。适合大规模 fan-out（多文件审计、批量迁移、多角度交叉研究）。中间结果留在脚本变量，只有 return 值进上下文。',
			promptGuidelines: [
				'任务需要超过 3 个并行子代理时，写 workflow 脚本调用本工具，而不是多次调用 subagent',
				'脚本内用 pipeline 做逐项 fan-out，用 constraints() 感知资源后定规模',
				'脚本内不能读写文件、不能起进程；读写让 agent() 去做',
			],
			parameters: Type.Object({
				script: Type.String({
					description: 'workflow 编排脚本（JavaScript，顶层 await 可用）',
				}),
			}),
			async execute(_id, params, _signal, _onUpdate, ctx) {
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
				const state = await runtime.run(params.script);
				if (state.status !== 'completed') {
					return {
						content: [
							{
								type: 'text' as const,
								text: `workflow ${state.status}: ${state.error ?? ''}`,
							},
						],
						details: undefined,
					};
				}
				const result =
					typeof state.result === 'string'
						? state.result
						: JSON.stringify(state.result);
				return {
					content: [
						{
							type: 'text' as const,
							text: `workflow 完成（${state.agentCalls} 次调用）:\n${result}`,
						},
					],
					details: undefined,
				};
			},
		});

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
	},
};
