// E2E: workflow 多文件审计——agent()/pipeline() fan-out，缓存恢复演示

import { readdirSync } from 'node:fs';
import { join } from 'node:path';
import { EventBus } from '@kxen/core';
import { WorkflowRuntime } from '@kxen/workflow';
import { buildStack } from './lib/stack';

const cwd = process.cwd();
const { manager } = await buildStack(cwd);
const bus = new EventBus();

const runtime = new WorkflowRuntime({
	bus,
	executeAgent: async (prompt, _spec, opts) => {
		const handle = await manager.spawn(
			{
				name: 'review',
				description: '审查',
				role: 'review',
				tools: ['read', 'exec'],
			},
			prompt,
			{ mode: 'background' },
		);
		return handle.result;
	},
	constraintsProvider: () => ({ note: 'e2e 演示' }),
});

const packagesDir = join(cwd, 'packages');
const packages = readdirSync(packagesDir, { withFileTypes: true })
	.filter((e) => e.isDirectory())
	.map((e) => e.name)
	.slice(0, 6);

console.log(`审计 ${packages.length} 个包: ${packages.join(', ')}`);

let realCalls = 0;
const countingBus = new EventBus();
const countingRuntime = new WorkflowRuntime({
	bus: countingBus,
	executeAgent: async (prompt) => {
		realCalls++;
		const handle = await manager.spawn(
			{
				name: 'review',
				description: '审查',
				role: 'review',
				tools: ['read', 'exec'],
			},
			prompt,
		);
		return handle.result;
	},
});

const script = `
	phase('audit')
	const findings = await pipeline(${JSON.stringify(packages)}, async (pkg) => {
		const r = await agent('检查 packages/' + pkg + ' 目录：index.ts 是否仍是 TODO 占位？有没有空的 src 文件？一句话回答。')
		return pkg + ': ' + r.summary.slice(0, 80)
	})
	return findings
`;

console.log('第一轮运行（真实调用）...');
const first = await countingRuntime.run(script);
console.log(`第一轮: status=${first.status} 真实 agent 调用=${realCalls}`);
console.log('报告预览:');
for (const line of (first.result as string[]).slice(0, 6))
	console.log(`  ${line}`);

const snap = countingRuntime.snapshot(first.id);
console.log('第二轮 resume（应全部回放，零真实调用）...');
realCalls = 0;
const resumed = await countingRuntime.run(script, undefined, {
	id: first.id,
	cache: snap?.cache ?? [],
});
console.log(`第二轮: status=${resumed.status} 真实 agent 调用=${realCalls}`);

const ok =
	first.status === 'completed' &&
	resumed.status === 'completed' &&
	realCalls === 0;
console.log(ok ? 'PASS workflow 端到端（fan-out + 缓存恢复）' : 'FAIL');
process.exit(ok ? 0 : 1);
