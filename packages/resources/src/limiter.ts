// token bucket：按账户真实限额的 80-90% 设桶，调用前扣估算量，把硬 429 变可控软延迟
export class TokenBucket {
	private tokens: number;
	private lastRefill: number;

	constructor(
		private capacity: number,
		private refillPerMs: number,
		private now: () => number = Date.now,
	) {
		this.tokens = capacity;
		this.lastRefill = this.now();
	}

	tryTake(n: number): boolean {
		const now = this.now();
		this.tokens = Math.min(
			this.capacity,
			this.tokens + (now - this.lastRefill) * this.refillPerMs,
		);
		this.lastRefill = now;
		if (this.tokens < n) return false;
		this.tokens -= n;
		return true;
	}

	get level(): number {
		return this.tokens;
	}
}

// AIMD 自适应并发：429 -> 减半；持续成功 -> +1；remaining <10% -> 主动降（Promptfoo 模式）
export class AimdController {
	current: number;

	constructor(
		readonly max: number,
		readonly min: number = 1,
	) {
		this.current = max;
	}

	onRateLimited(): number {
		this.current = Math.max(this.min, Math.floor(this.current / 2));
		return this.current;
	}

	onSuccess(): number {
		this.current = Math.min(this.max, this.current + 1);
		return this.current;
	}

	onRemainingLow(remainingRatio: number): number {
		if (remainingRatio < 0.1) {
			this.current = Math.max(this.min, Math.floor(this.current / 2));
		} else if (remainingRatio < 0.2 && this.current > this.min + 1) {
			this.current = Math.max(this.min, this.current - 1);
		}
		return this.current;
	}
}

export interface WaitInput {
	retryAfterSec?: number;
	resetAtMs?: number;
	attempt: number;
	baseMs?: number;
	maxMs?: number;
	now?: number;
	jitter?: () => number;
}

// 三级等待：retry-after 优先 -> reset 头换算 -> 全抖动指数退避
export function computeWaitMs(input: WaitInput): number {
	const now = input.now ?? Date.now();
	const jitter = input.jitter ?? Math.random;
	if (input.retryAfterSec !== undefined && input.retryAfterSec > 0) {
		return input.retryAfterSec * 1000 + Math.floor(jitter() * 250);
	}
	if (input.resetAtMs !== undefined && input.resetAtMs > now) {
		return input.resetAtMs - now + Math.floor(jitter() * 250);
	}
	const base = input.baseMs ?? 500;
	const max = input.maxMs ?? 60_000;
	const exp = Math.min(max, base * 2 ** input.attempt);
	return Math.floor(jitter() * exp);
}
