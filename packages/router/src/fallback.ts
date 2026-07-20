export interface ProviderHealth {
	consecutiveFailures: number;
	cooldownUntil?: number;
}

export class HealthTracker {
	private health = new Map<string, ProviderHealth>();

	constructor(private cooldownMs: number = 60_000) {}

	markFailure(providerId: string, now: number): void {
		const h = this.health.get(providerId) ?? { consecutiveFailures: 0 };
		h.consecutiveFailures++;
		h.cooldownUntil = now + this.cooldownMs;
		this.health.set(providerId, h);
	}

	markSuccess(providerId: string): void {
		this.health.set(providerId, { consecutiveFailures: 0 });
	}

	isCoolingDown(providerId: string, now: number): boolean {
		const h = this.health.get(providerId);
		if (!h?.cooldownUntil) return false;
		// cooldown-expiry：到期自动恢复资格
		return h.cooldownUntil > now;
	}

	state(providerId: string): ProviderHealth {
		return this.health.get(providerId) ?? { consecutiveFailures: 0 };
	}
}

// 链选择：跳过冷却中的 provider，取第一个健康项（exact -> provider/* -> role -> default 由调用方组装顺序）
export function selectFromChain(
	chain: string[],
	isAvailable: (model: string) => boolean,
): string | undefined {
	return chain.find(isAvailable);
}
