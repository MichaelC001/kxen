import type { SubagentResult, SubagentSpec } from '@kxen/subagent';

export interface AgentCallOptions {
	role?: string;
	label?: string;
	schema?: Record<string, unknown>;
}

export interface PipelineOptions {
	concurrency?: number;
	label?: string;
}

// 脚本可见的 API 面（design/03）：agent / pipeline / constraints / phase
export interface WorkflowApi {
	agent(prompt: string, opts?: AgentCallOptions): Promise<SubagentResult>;
	pipeline<T, R>(
		items: T[],
		fn: (item: T, index: number) => Promise<R>,
		opts?: PipelineOptions,
	): Promise<R[]>;
	constraints(): Record<string, unknown>;
	phase(name: string): void;
	readonly args: unknown;
}

export interface WorkflowRunState {
	id: string;
	status: 'running' | 'paused' | 'completed' | 'failed' | 'stopped';
	phases: {
		name: string;
		agentCount: number;
		startedAt: number;
		endedAt?: number;
	}[];
	agentCalls: number;
	result?: unknown;
	error?: string;
}

export type AgentExecutor = (
	prompt: string,
	spec: SubagentSpec | undefined,
	opts: AgentCallOptions,
) => Promise<SubagentResult>;

export type ConstraintsProvider = () => Record<string, unknown>;
