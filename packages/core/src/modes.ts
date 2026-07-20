export type Mode = 'plan' | 'build';

export interface ModeSpec {
	allowedTools: string[];
	execReadOnly: boolean;
}

// plan: 只读（read + exec 只读白名单）；build: 全工具
export const MODES: Record<Mode, ModeSpec> = {
	plan: { allowedTools: ['read', 'exec'], execReadOnly: true },
	build: {
		allowedTools: ['read', 'write', 'edit', 'exec'],
		execReadOnly: false,
	},
};

// plan 模式 exec 只读命令白名单（前缀匹配）
export const PLAN_EXEC_WHITELIST = [
	'ls',
	'pwd',
	'git status',
	'git log',
	'git diff',
	'git show',
	'git branch',
	'rg',
	'grep',
	'find',
	'head',
	'tail',
	'wc',
	'file',
	'which',
	'echo',
];

export function isPlanExecAllowed(command: string): boolean {
	const head = command.trim().split(/[|;&]/)[0]?.trim() ?? '';
	return PLAN_EXEC_WHITELIST.some(
		(prefix) => head === prefix || head.startsWith(`${prefix} `),
	);
}
