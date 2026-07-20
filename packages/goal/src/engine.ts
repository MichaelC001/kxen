import type { EventBus } from '@kxen/core';
import type { Goal, GoalContract, TurnOutcome } from './types';

export interface GoalEngineOptions {
	bus: EventBus;
	now?: () => number;
}

const TERMINAL_STATUSES = new Set([
	'complete',
	'blocked',
	'budget_limited',
	'canceled',
]);

// goal 引擎：状态机 + 队列 + 验证循环 + blocked 三次规则（research/05、design/03）
export class GoalEngine {
	private goals = new Map<string, Goal>();
	private nextId = 1;

	constructor(private opts: GoalEngineOptions) {}

	private now(): number {
		return this.opts.now?.() ?? Date.now();
	}

	create(contract: GoalContract, priority = 0): Goal {
		if (!contract.objective || !contract.completionCriteria) {
			throw new Error(
				'goal 必须有 objective 与 completionCriteria（completion contract）',
			);
		}
		const goal: Goal = {
			id: `goal-${this.nextId++}`,
			contract,
			status: 'draft',
			priority,
			createdAt: this.now(),
			updatedAt: this.now(),
			turnsUsed: 0,
			tokensUsed: 0,
			consecutiveBlocks: 0,
		};
		this.goals.set(goal.id, goal);
		this.opts.bus.publish('goal.created', {
			id: goal.id,
			objective: contract.objective,
		});
		return goal;
	}

	// 单活跃：激活一个，其余进队列按优先级排序
	activate(id: string): Goal {
		const goal = this.mustGet(id);
		for (const g of this.goals.values()) {
			if (g.status === 'active' && g.id !== id) {
				g.status = 'queued';
				g.updatedAt = this.now();
			}
		}
		goal.status = 'active';
		goal.updatedAt = this.now();
		this.opts.bus.publish('goal.activated', { id });
		return goal;
	}

	activeGoal(): Goal | undefined {
		return [...this.goals.values()].find((g) => g.status === 'active');
	}

	pause(id: string): void {
		const goal = this.mustGet(id);
		if (goal.status !== 'active') return;
		goal.status = 'paused';
		goal.updatedAt = this.now();
		this.opts.bus.publish('goal.paused', { id });
	}

	resume(id: string): void {
		this.activate(id);
	}

	cancel(id: string): void {
		const goal = this.mustGet(id);
		goal.status = 'canceled';
		goal.updatedAt = this.now();
		this.opts.bus.publish('goal.canceled', { id });
	}

	get(id: string): Goal | undefined {
		return this.goals.get(id);
	}

	list(): Goal[] {
		return [...this.goals.values()].sort(
			(a, b) => b.priority - a.priority || a.createdAt - b.createdAt,
		);
	}

	private mustGet(id: string): Goal {
		const goal = this.goals.get(id);
		if (!goal) throw new Error(`goal 不存在: ${id}`);
		return goal;
	}

	private isBudgetExceeded(goal: Goal): boolean {
		const b = goal.contract.budget;
		if (!b) return false;
		if (b.turns !== undefined && goal.turnsUsed >= b.turns) return true;
		if (b.tokens !== undefined && goal.tokensUsed >= b.tokens) return true;
		if (
			b.wallClockMs !== undefined &&
			this.now() - goal.createdAt >= b.wallClockMs
		)
			return true;
		return false;
	}

	// 执行一轮：outcome 由上层（会话 / 编排）产出；引擎只判定状态推进
	applyTurn(id: string, outcome: TurnOutcome): Goal {
		const goal = this.mustGet(id);
		if (TERMINAL_STATUSES.has(goal.status)) return goal;

		goal.turnsUsed++;
		goal.tokensUsed += outcome.tokensUsed ?? 0;
		goal.updatedAt = this.now();

		if (outcome.blockedReason) {
			if (outcome.terminal) {
				goal.status = 'blocked';
				goal.blockReason = outcome.blockedReason;
				this.opts.bus.publish('goal.blocked', {
					id,
					reason: outcome.blockedReason,
					terminal: true,
				});
				return goal;
			}
			if (goal.lastBlockReason === outcome.blockedReason) {
				goal.consecutiveBlocks++;
			} else {
				goal.lastBlockReason = outcome.blockedReason;
				goal.consecutiveBlocks = 1;
			}
			if (goal.consecutiveBlocks >= 3) {
				goal.status = 'blocked';
				goal.blockReason = outcome.blockedReason;
				this.opts.bus.publish('goal.blocked', {
					id,
					reason: outcome.blockedReason,
					terminal: false,
				});
			}
		} else {
			goal.consecutiveBlocks = 0;
			goal.lastBlockReason = undefined;
		}

		if (goal.status === 'active' && this.isBudgetExceeded(goal)) {
			goal.status = 'budget_limited';
			this.opts.bus.publish('goal.budget_limited', {
				id,
				turns: goal.turnsUsed,
				tokens: goal.tokensUsed,
			});
		}
		return goal;
	}

	// 验证通过：只允许在证据存在时 complete
	complete(id: string, evidence: string): Goal {
		const goal = this.mustGet(id);
		if (!evidence) throw new Error('complete 需要验证证据，不允许空证据完成');
		goal.status = 'complete';
		goal.verificationEvidence = evidence;
		goal.updatedAt = this.now();
		this.opts.bus.publish('goal.completed', { id, evidence });
		this.dequeueNext();
		return goal;
	}

	private dequeueNext(): void {
		const next = this.list().find(
			(g) => g.status === 'queued' || g.status === 'draft',
		);
		if (next) this.activate(next.id);
	}
}
