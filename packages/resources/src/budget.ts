import type { EventBus } from '@kxen/core';

export interface BudgetLimits {
	tokens?: number;
	costUsd?: number;
}

export interface BudgetUsage {
	tokens: number;
	costUsd: number;
}

// 会话级预算账户：80% / 95% 两档水位事件（analysis/03）
export class BudgetAccount {
	private usage: BudgetUsage = { tokens: 0, costUsd: 0 };
	private fired80 = false;
	private fired95 = false;

	constructor(
		private limits: BudgetLimits,
		private bus?: EventBus,
	) {}

	record(delta: { tokens?: number; costUsd?: number }): void {
		this.usage.tokens += delta.tokens ?? 0;
		this.usage.costUsd += delta.costUsd ?? 0;
		const wm = this.watermark();
		if (wm >= 0.95 && !this.fired95) {
			this.fired95 = true;
			this.bus?.publish('budget.critical', {
				watermark: wm,
				usage: this.usage,
			});
		} else if (wm >= 0.8 && !this.fired80) {
			this.fired80 = true;
			this.bus?.publish('budget.warning', { watermark: wm, usage: this.usage });
		}
	}

	watermark(): number {
		let wm = 0;
		if (this.limits.tokens && this.limits.tokens > 0) {
			wm = Math.max(wm, this.usage.tokens / this.limits.tokens);
		}
		if (this.limits.costUsd && this.limits.costUsd > 0) {
			wm = Math.max(wm, this.usage.costUsd / this.limits.costUsd);
		}
		return wm;
	}

	exhausted(): boolean {
		return this.watermark() >= 1;
	}

	get current(): BudgetUsage {
		return { ...this.usage };
	}
}
