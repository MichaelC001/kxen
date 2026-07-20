export type GoalStatus =
	| 'draft'
	| 'queued'
	| 'active'
	| 'paused'
	| 'complete'
	| 'blocked'
	| 'budget_limited'
	| 'canceled';

export interface GoalBudget {
	tokens?: number;
	turns?: number;
	wallClockMs?: number;
}

// completion contract：缺了不许建（research/05）
export interface GoalContract {
	objective: string;
	completionCriteria: string;
	constraints?: string;
	budget?: GoalBudget;
}

export interface ExecutableCheck {
	type: 'command';
	run: string;
	cwd?: string;
}

export interface Goal {
	id: string;
	contract: GoalContract;
	status: GoalStatus;
	priority: number;
	createdAt: number;
	updatedAt: number;
	turnsUsed: number;
	tokensUsed: number;
	// blocked 三次规则：同一阻塞原因连续计数
	lastBlockReason?: string;
	consecutiveBlocks: number;
	blockReason?: string;
	verificationEvidence?: string;
}

export interface TurnOutcome {
	// 本轮做了什么（摘要）
	summary: string;
	tokensUsed?: number;
	// 若判定无法继续：阻塞原因（同一原因累计 3 次才允许 blocked，除非不可能/不安全/矛盾）
	blockedReason?: string;
	// 是否终态阻塞（不可能 / 不安全 / 矛盾 -> 当轮即可 blocked）
	terminal?: boolean;
}
