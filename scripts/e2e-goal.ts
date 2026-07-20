// E2E: goal 模式端到端——创建 goal，多轮执行直到可执行验证通过

import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { EventBus, KxenSession } from '@kxen/core';
import { GoalEngine, runGoal } from '@kxen/goal';
import { createKxenTools } from '@kxen/tools';
import { buildStack } from './lib/stack';

const cwd = process.cwd();
const { mrm, bus } = await buildStack(cwd);

const engine = new GoalEngine({ bus });
const goal = engine.create({
	objective:
		'在 packages/git/src/git.ts 中新增一个导出函数 gitLog(cwd: string, n: number): Promise<string[]>，返回最近 n 条提交的 oneline 字符串数组。不要修改其他文件，不要改现有函数。',
	completionCriteria:
		'bunx tsc -p tsconfig.base.json --noEmit 退出码为 0，且 packages/git/src/git.ts 包含导出函数 gitLog',
	constraints: '只改 packages/git/src/git.ts',
	budget: { turns: 6 },
});

const session = await KxenSession.create({
	cwd,
	tools: createKxenTools(),
	bus,
});

// 主会话显式用 anthropic（models.json 注入 kimi 后 "first available" 顺序不稳定）
const { ModelRegistry } = await import('@earendil-works/pi-coding-agent');
const registry = new ModelRegistry(session.inner.modelRuntime);
const claude = registry.getAvailable().find((m) => m.provider === 'anthropic');
if (claude) await session.inner.setModel(claude);

async function verify(): Promise<{ ok: boolean; evidence: string }> {
	const content = readFileSync(join(cwd, 'packages/git/src/git.ts'), 'utf8');
	if (!content.includes('gitLog'))
		return { ok: false, evidence: 'gitLog 尚未出现' };
	const proc = Bun.spawn(
		['bunx', 'tsc', '-p', 'tsconfig.base.json', '--noEmit'],
		{
			cwd,
			stdout: 'pipe',
			stderr: 'pipe',
		},
	);
	const code = await proc.exited;
	if (code === 0)
		return { ok: true, evidence: 'tsc 退出码 0 且 gitLog 已导出' };
	return { ok: false, evidence: `tsc 退出码 ${code}` };
}

console.log(
	`goal 创建: ${goal.id} -> ${goal.contract.objective.slice(0, 60)}...`,
);
const final = await runGoal(engine, goal.id, {
	maxTurns: 6,
	verify: async () => verify(),
	executeTurn: async (g, turn) => {
		console.log(`第 ${turn + 1} 轮执行...`);
		const slot = await mrm.acquire({ role: 'execution', priority: 5 });
		try {
			await session.prompt(
				`请完成这个任务（只改 packages/git/src/git.ts）：${g.contract.objective}\n完成后不要解释，直接结束。`,
			);
			return { summary: 'agent 执行一轮' };
		} finally {
			mrm.release(slot, { ok: true });
		}
	},
});

console.log(`goal 最终状态: ${final.status}`);
if (final.verificationEvidence)
	console.log(`验证证据: ${final.verificationEvidence}`);
const ok = final.status === 'complete';
console.log(
	ok ? 'PASS goal 端到端' : `FAIL (${final.blockReason ?? final.status})`,
);
process.exit(ok ? 0 : 1);
