import type { EventBus } from './events';
import type { MemorySample } from './telemetry';

export type MemoryGuardAction =
	| 'ok'
	| 'evict_display'
	| 'request_compaction'
	| 'reject_new';

// E1 内存水位 watchdog：display 驱逐 -> 提前压缩 -> 拒绝新调用
export class MemoryGuard {
	constructor(
		private opts: {
			warnBytes: number;
			criticalBytes: number;
			bus?: EventBus;
		},
	) {}

	check(sample: MemorySample): MemoryGuardAction[] {
		const actions: MemoryGuardAction[] = [];
		if (sample.heapUsed >= this.opts.criticalBytes) {
			actions.push('evict_display', 'request_compaction', 'reject_new');
			this.opts.bus?.publish('memory.critical', { heapUsed: sample.heapUsed });
		} else if (sample.heapUsed >= this.opts.warnBytes) {
			actions.push('evict_display');
			this.opts.bus?.publish('memory.warn', { heapUsed: sample.heapUsed });
		}
		if (actions.length === 0) actions.push('ok');
		return actions;
	}
}
