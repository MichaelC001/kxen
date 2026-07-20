export interface SubagentSpec {
	name: string;
	description: string;
	// 角色（模型路由用）：explore / plan / execute / review / 自定义
	role: string;
	// 工具白名单（schema 级过滤，看不到即不会误用）
	tools?: string[];
	// 模型覆盖（可选，默认按 role 路由）
	model?: string;
}

export interface SubagentResult {
	summary: string;
	filesChanged: string[];
	stopReason: 'completed' | 'stopped' | 'error';
	usage?: { input?: number; output?: number };
	provider?: string;
	model?: string;
	error?: string;
}

export type SpawnMode = 'foreground' | 'background';

export interface SpawnOptions {
	mode?: SpawnMode;
	// 隔离方式：worktree = git worktree 隔离；none = 共享 cwd
	isolation?: 'worktree' | 'none';
	timeoutMs?: number;
}

export interface SubagentHandle {
	id: string;
	spec: SubagentSpec;
	result: Promise<SubagentResult>;
	steer(text: string): Promise<void>;
	stop(reason?: string): Promise<void>;
}

// 内置四型（design/07）
export const BUILTIN_SUBAGENTS: SubagentSpec[] = [
	{
		name: 'explore',
		description: '只读研究：代码检索、结构分析、问题定位',
		role: 'research',
		tools: ['read', 'exec'],
	},
	{
		name: 'plan',
		description: '只读规划：任务拆解、方案设计',
		role: 'planning',
		tools: ['read', 'exec'],
	},
	{
		name: 'execute',
		description: '执行型：编辑与命令',
		role: 'execution',
		tools: ['read', 'write', 'edit', 'exec'],
	},
	{
		name: 'review',
		description: '审查型：对抗复核、验证',
		role: 'review',
		tools: ['read', 'exec'],
	},
];
