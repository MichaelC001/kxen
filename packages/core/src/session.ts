import type {
	AgentSession,
	AgentSessionEvent,
	ToolDefinition,
} from '@earendil-works/pi-coding-agent';
import { createAgentSession } from '@earendil-works/pi-coding-agent';
import { EventBus } from './events';
import { ensureAgentDir } from './provider';

export interface KxenSessionOptions {
	cwd: string;
	tools: ToolDefinition[];
	allowedTools?: string[];
	bus?: EventBus;
	agentDir?: string;
}

// E2 三层分离骨架：context 由 pi session 管理；display 是有界展示层；storage 交给 SessionManager
export class KxenSession {
	private constructor(
		readonly inner: AgentSession,
		readonly bus: EventBus,
		private unsubscribe: () => void,
	) {}

	static async create(opts: KxenSessionOptions): Promise<KxenSession> {
		const bus = opts.bus ?? new EventBus();
		const { session } = await createAgentSession({
			cwd: opts.cwd,
			agentDir: opts.agentDir ?? ensureAgentDir(),
			customTools: opts.tools,
			...(opts.allowedTools ? { tools: opts.allowedTools } : {}),
		});
		const unsubscribe = session.subscribe((event: AgentSessionEvent) => {
			bus.publish(`session.${event.type}`, safePayload(event));
		});
		return new KxenSession(session, bus, unsubscribe);
	}

	async prompt(text: string): Promise<void> {
		this.bus.publish('kxen.prompt', { text });
		await this.inner.prompt(text);
	}

	async dispose(): Promise<void> {
		this.unsubscribe();
	}
}

// 事件负载进总线前剥掉大体内容（diff / 完整消息体），防止 display 层膨胀（E2 / E5）
function safePayload(event: AgentSessionEvent): Record<string, unknown> {
	const e = event as Record<string, unknown>;
	const out: Record<string, unknown> = { type: e.type };
	for (const key of ['reason', 'name', 'level', 'willRetry']) {
		if (key in e) out[key] = e[key];
	}
	return out;
}
