import type { ToolDefinition } from '@earendil-works/pi-coding-agent';
import type { EventBus, KxenSession } from '@kxen/core';
import {
	collectChangedFiles,
	createWorktree,
	removeWorktree,
	type WorktreeInfo,
} from '@kxen/git';
import type { ModelResourceManager, Slot } from '@kxen/resources';
import type {
	SpawnOptions,
	SubagentHandle,
	SubagentResult,
	SubagentSpec,
} from './types';

export type SessionFactory = (opts: {
	cwd: string;
	tools: ToolDefinition[];
	allowedTools?: string[];
	bus: EventBus;
}) => Promise<KxenSession>;

export interface SubagentManagerOptions {
	mrm: ModelResourceManager;
	bus: EventBus;
	createSession: SessionFactory;
	toolsFor: (spec: SubagentSpec) => ToolDefinition[];
	repoRoot: string;
}

interface RunningEntry {
	session: KxenSession;
	slot: Slot;
	worktree?: WorktreeInfo;
	stopReason?: string;
}

// 子代理管理：spawn / swarm / typed 结果 / worktree 隔离 / steering（design/07）
export class SubagentManager {
	private running = new Map<string, RunningEntry>();
	private nextId = 1;

	constructor(private opts: SubagentManagerOptions) {}

	async spawn(
		spec: SubagentSpec,
		prompt: string,
		opts: SpawnOptions = {},
	): Promise<SubagentHandle> {
		const id = `sa-${this.nextId++}`;
		const slot = await this.opts.mrm.acquire({ role: spec.role, priority: 5 });
		this.opts.bus.publish('subagent.spawn', {
			id,
			spec,
			mode: opts.mode ?? 'foreground',
		});

		let worktree: WorktreeInfo | undefined;
		let session: KxenSession | undefined;
		try {
			if (opts.isolation === 'worktree') {
				worktree = await createWorktree(this.opts.repoRoot, id);
			}
			session = await this.opts.createSession({
				cwd: worktree?.path ?? this.opts.repoRoot,
				tools: this.opts.toolsFor(spec),
				allowedTools: spec.tools,
				bus: this.opts.bus,
			});
		} catch (err) {
			if (worktree) await removeWorktree(this.opts.repoRoot, worktree);
			this.opts.mrm.release(slot, { ok: false, error: err });
			throw err;
		}

		const entry: RunningEntry = { session, slot, worktree };
		this.running.set(id, entry);

		// 按 slot 路由结果设置模型（多订阅混用：execution -> xai、grok 等）
		if (slot.model && slot.model !== 'default') {
			await this.applySlotModel(session, slot.model).catch(() => {});
		}

		const result = this.run(id, spec, prompt, entry);
		const handle: SubagentHandle = {
			id,
			spec,
			result,
			steer: async (text: string) => {
				this.opts.bus.publish('subagent.steer', { id, text });
				await session.inner.steer(text);
			},
			stop: async (reason = 'manual') => {
				entry.stopReason = reason;
				this.opts.bus.publish('subagent.stop', { id, reason });
				await session.dispose();
			},
		};
		return handle;
	}

	private async run(
		id: string,
		spec: SubagentSpec,
		prompt: string,
		entry: RunningEntry,
	): Promise<SubagentResult> {
		const { session, slot, worktree } = entry;
		try {
			await session.prompt(prompt);
			const summary = lastAssistantText(session);
			const filesChanged = worktree
				? await collectChangedFiles(worktree.path)
				: [];
			const usage = lastUsage(session);
			const modelInfo = lastModelInfo(session);
			this.opts.bus.publish('subagent.complete', {
				id,
				spec,
				filesChanged: filesChanged.length,
			});
			return {
				summary,
				filesChanged,
				stopReason: entry.stopReason ? 'stopped' : 'completed',
				usage,
				provider: modelInfo?.provider,
				model: modelInfo?.model,
			};
		} catch (err) {
			const message = err instanceof Error ? err.message : String(err);
			this.opts.bus.publish('subagent.error', { id, spec, error: message });
			return {
				summary: '',
				filesChanged: [],
				stopReason: 'error',
				error: message,
			};
		} finally {
			this.running.delete(id);
			if (worktree)
				await removeWorktree(this.opts.repoRoot, worktree).catch(() => {});
			this.opts.mrm.release(slot, { ok: !entry.stopReason });
			await session.dispose().catch(() => {});
		}
	}

	// swarm：同模板批量，item 间 scope 必须互不冲突，数量由 MRM 调度
	async swarm(
		spec: SubagentSpec,
		items: string[],
		opts: SpawnOptions = {},
	): Promise<SubagentHandle[]> {
		const handles: SubagentHandle[] = [];
		for (const item of items) {
			handles.push(await this.spawn(spec, item, opts));
		}
		return handles;
	}

	get runningCount(): number {
		return this.running.size;
	}

	// slot.model 形如 'provider/model-id'，在注册表中解析并 setModel；解析不到保持默认
	private async applySlotModel(
		session: KxenSession,
		modelRef: string,
	): Promise<void> {
		const { ModelRegistry } = await import('@earendil-works/pi-coding-agent');
		const [provider, ...rest] = modelRef.split('/');
		const modelId = rest.join('/');
		const registry = new ModelRegistry(session.inner.modelRuntime);
		const model =
			(provider ? registry.find(provider, modelId) : undefined) ??
			registry.getAvailable().find((m) => m.id === modelId);
		if (model) await session.inner.setModel(model);
		this.opts.bus.publish('subagent.model_selected', {
			model: modelRef,
			resolved: !!model,
		});
	}
}

function lastAssistantText(session: KxenSession): string {
	const messages = session.inner.agent.state.messages as Array<{
		role?: string;
		content?: Array<{ type?: string; text?: string }> | string;
	}>;
	const last = [...messages].reverse().find((m) => m.role === 'assistant');
	if (!last) return '';
	if (typeof last.content === 'string') return last.content;
	return (last.content ?? [])
		.filter((c) => c.type === 'text')
		.map((c) => c.text)
		.join('\n');
}

function lastUsage(session: KxenSession): SubagentResult['usage'] {
	const messages = session.inner.agent.state.messages as Array<{
		role?: string;
		usage?: { input?: number; output?: number };
	}>;
	const last = [...messages]
		.reverse()
		.find((m) => m.role === 'assistant' && m.usage);
	return last?.usage;
}

function lastModelInfo(session: KxenSession): {
	provider?: string;
	model?: string;
} {
	const messages = session.inner.agent.state.messages as Array<{
		role?: string;
		provider?: string;
		model?: string;
	}>;
	const last = [...messages].reverse().find((m) => m.role === 'assistant');
	return { provider: last?.provider, model: last?.model };
}
