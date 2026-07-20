import { createWriteStream, mkdirSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { pipeline } from 'node:stream/promises';
import { getHeapSnapshot } from 'node:v8';

export interface MemorySample {
	ts: number;
	rss: number;
	heapUsed: number;
	heapTotal: number;
	external: number;
}

export interface TelemetryOptions {
	intervalMs?: number;
	dumpDir?: string;
	onSample?: (sample: MemorySample) => void;
	now?: () => number;
}

export function takeSample(now: () => number = Date.now): MemorySample {
	const m = process.memoryUsage();
	return {
		ts: now(),
		rss: m.rss,
		heapUsed: m.heapUsed,
		heapTotal: m.heapTotal,
		external: m.external,
	};
}

export async function writeHeapDump(
	dumpDir: string,
	now: () => number = Date.now,
): Promise<string> {
	mkdirSync(dumpDir, { recursive: true });
	const path = join(dumpDir, `heap-${now()}.heapsnapshot`);
	await pipeline(getHeapSnapshot(), createWriteStream(path));
	return path;
}

// 返回停止函数；SIGUSR1 触发 heap dump（E8）
export function startMemoryTelemetry(opts: TelemetryOptions = {}): () => void {
	const intervalMs = opts.intervalMs ?? 30_000;
	const dumpDir = opts.dumpDir ?? join(homedir(), '.kxen', 'dumps');
	const timer = setInterval(() => {
		opts.onSample?.(takeSample(opts.now));
	}, intervalMs);
	timer.unref();
	const onSigusr1 = () => {
		void writeHeapDump(dumpDir, opts.now).catch(() => {
			// dump 失败不影响进程
		});
	};
	process.on('SIGUSR1', onSigusr1);
	return () => {
		clearInterval(timer);
		process.off('SIGUSR1', onSigusr1);
	};
}
