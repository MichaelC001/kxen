import type { EventBus, LimitsConfig } from '@kxen/core';
import {
	HealthTracker,
	providerOf,
	type RolesConfig,
	resolveRole,
} from '@kxen/router';

export interface AcquireRequest {
	role: string;
	estimatedTokens?: number;
	priority?: number;
}

export interface Slot {
	id: number;
	role: string;
	model: string;
	providerId: string;
}

export interface SlotOutcome {
	ok: boolean;
	usage?: { input?: number; output?: number };
	error?: unknown;
}

export interface MRMStatus {
	global: { inFlight: number; max: number };
	providers: Record<
		string,
		{ inFlight: number; max: number; coolingDown: boolean }
	>;
	roles: Record<string, { inFlight: number; max: number }>;
	queued: number;
}

interface QueuedRequest {
	req: AcquireRequest;
	resolve: (slot: Slot) => void;
	reject: (err: Error) => void;
}

// 全局唯一入口：所有模型调用经过 acquire -> use -> release（analysis/03）
export class ModelResourceManager {
	private inFlightGlobal = 0;
	private inFlightByProvider = new Map<string, number>();
	private inFlightByRole = new Map<string, number>();
	private queue: QueuedRequest[] = [];
	private nextId = 1;
	readonly health: HealthTracker;

	constructor(
		private opts: {
			roles: RolesConfig;
			limits: LimitsConfig;
			bus?: EventBus;
			now?: () => number;
		},
	) {
		this.health = new HealthTracker();
	}

	private now(): number {
		return this.opts.now?.() ?? Date.now();
	}

	private maxGlobal(): number {
		return this.opts.limits.global?.concurrent ?? 16;
	}
	private maxProvider(providerId: string): number {
		return this.opts.limits.providers?.[providerId]?.concurrent ?? 4;
	}
	private maxRole(role: string): number {
		return this.opts.limits.roles?.[role]?.concurrent ?? 8;
	}

	private inFlightProvider(providerId: string): number {
		return this.inFlightByProvider.get(providerId) ?? 0;
	}
	private inFlightRole(role: string): number {
		return this.inFlightByRole.get(role) ?? 0;
	}

	private canAdmit(role: string, providerId: string): boolean {
		if (this.inFlightGlobal >= this.maxGlobal()) return false;
		if (this.inFlightProvider(providerId) >= this.maxProvider(providerId))
			return false;
		if (this.inFlightRole(role) >= this.maxRole(role)) return false;
		if (this.health.isCoolingDown(providerId, this.now())) return false;
		return true;
	}

	// 角色路由 + 健康过滤：从首选链里选当前可接纳的模型
	private pickModel(role: string): string | undefined {
		const resolved = resolveRole(this.opts.roles, role);
		if (!resolved) return undefined;
		for (const candidate of [resolved.primary, ...resolved.chain]) {
			const providerId = providerOf(candidate.model);
			if (this.canAdmit(role, providerId)) return candidate.model;
		}
		return undefined;
	}

	async acquire(req: AcquireRequest): Promise<Slot> {
		const immediate = this.tryAcquire(req);
		if (immediate) return immediate;
		return new Promise<Slot>((resolve, reject) => {
			this.queue.push({ req, resolve, reject });
			this.queue.sort((a, b) => (b.req.priority ?? 0) - (a.req.priority ?? 0));
		});
	}

	private tryAcquire(req: AcquireRequest): Slot | undefined {
		const model = this.pickModel(req.role);
		if (!model) return undefined;
		const providerId = providerOf(model);
		this.inFlightGlobal++;
		this.inFlightByProvider.set(
			providerId,
			this.inFlightProvider(providerId) + 1,
		);
		this.inFlightByRole.set(req.role, this.inFlightRole(req.role) + 1);
		const slot: Slot = { id: this.nextId++, role: req.role, model, providerId };
		this.opts.bus?.publish('mrm.acquired', { slot });
		return slot;
	}

	release(slot: Slot, outcome: SlotOutcome): void {
		this.inFlightGlobal = Math.max(0, this.inFlightGlobal - 1);
		this.inFlightByProvider.set(
			slot.providerId,
			Math.max(0, this.inFlightProvider(slot.providerId) - 1),
		);
		this.inFlightByRole.set(
			slot.role,
			Math.max(0, this.inFlightRole(slot.role) - 1),
		);
		if (outcome.ok) {
			this.health.markSuccess(slot.providerId);
		} else {
			this.health.markFailure(slot.providerId, this.now());
			this.opts.bus?.publish('mrm.provider_cooldown', {
				providerId: slot.providerId,
			});
		}
		this.opts.bus?.publish('mrm.released', {
			slot,
			ok: outcome.ok,
			usage: outcome.usage,
		});
		this.drain();
	}

	private drain(): void {
		while (this.queue.length > 0) {
			const head = this.queue[0];
			if (!head) return;
			const slot = this.tryAcquire(head.req);
			if (!slot) return;
			this.queue.shift();
			head.resolve(slot);
		}
	}

	status(): MRMStatus {
		const providers: MRMStatus['providers'] = {};
		for (const providerId of new Set([
			...Object.keys(this.opts.limits.providers ?? {}),
			...this.inFlightByProvider.keys(),
		])) {
			providers[providerId] = {
				inFlight: this.inFlightProvider(providerId),
				max: this.maxProvider(providerId),
				coolingDown: this.health.isCoolingDown(providerId, this.now()),
			};
		}
		const roles: MRMStatus['roles'] = {};
		for (const role of new Set([
			...Object.keys(this.opts.roles),
			...this.inFlightByRole.keys(),
		])) {
			roles[role] = {
				inFlight: this.inFlightRole(role),
				max: this.maxRole(role),
			};
		}
		return {
			global: { inFlight: this.inFlightGlobal, max: this.maxGlobal() },
			providers,
			roles,
			queued: this.queue.length,
		};
	}
}
