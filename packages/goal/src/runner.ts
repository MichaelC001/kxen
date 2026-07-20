import type { EventBus } from '@kxen/core';
import type { GoalEngine } from './engine';
import type { Goal } from './types';

export interface GoalRunnerDeps {
	// 执行一轮（驱动 agent 干活），返回本轮摘要；可调用方注入会话/子代理
	executeTurn: (
		goal: Goal,
		turnIndex: number,
	) => Promise<{
		summary: string;
		tokensUsed?: number;
		blockedReason?: string;
		terminal?: boolean;
	}>;
	// 可执行验证（design/03：completion 走可执行检查，不走模型自评）
	verify: (goal: Goal) => Promise<{ ok: boolean; evidence: string }>;
	maxTurns?: number;
	bus?: EventBus;
}

// goal runner：创建 -> 激活 -> 循环（执行 -> 验证 -> 状态推进）直到完成 / 阻塞 / 预算耗尽
export async function runGoal(
	engine: GoalEngine,
	goalId: string,
	deps: GoalRunnerDeps,
): Promise<Goal> {
	const goal = engine.get(goalId);
	if (!goal) throw new Error(`goal 不存在: ${goalId}`);
	engine.activate(goalId);
	const maxTurns = deps.maxTurns ?? 20;

	for (let turn = 0; turn < maxTurns; turn++) {
		const current = engine.get(goalId);
		if (!current || current.status !== 'active') break;

		const verification = await deps.verify(current);
		if (verification.ok) {
			engine.complete(goalId, verification.evidence);
			return engine.get(goalId) as Goal;
		}

		const outcome = await deps.executeTurn(current, turn);
		engine.applyTurn(goalId, outcome);

		const after = engine.get(goalId);
		if (after && after.status !== 'active') break;
	}

	return engine.get(goalId) as Goal;
}
