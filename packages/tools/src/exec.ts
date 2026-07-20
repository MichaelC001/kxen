import { mkdirSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';

export type ShellType = 'zsh' | 'bash' | 'fish' | 'cmd' | 'powershell';

export interface ExecInput {
	type: ShellType;
	path: string;
	command: string;
	timeout?: number;
	background?: boolean;
}

export interface ExecResult {
	exitCode: number;
	stdout: string;
	truncated: boolean;
	outputFile?: string;
}

export const DEFAULT_TIMEOUT_MS = 120_000;
export const MAX_OUTPUT_BYTES = 30_000;

// 方言卡片：随工具描述下发，模型按 type 自查语法（X2）
export const DIALECT_CARDS: Record<ShellType, string> = {
	zsh: 'zsh: 数组下标从 1 开始；$VAR 与 ${VAR} 均可；支持 && 与 ||',
	bash: 'bash: 数组下标从 0 开始；支持 && 与 ||；shopt 扩展默认关',
	fish: 'fish: 变量导出用 set -x NAME value（不是 export）；支持 &&（3.0+）',
	cmd: 'cmd: 不支持 &&（老版本）；路径分隔符 \\；无单引号语义',
	powershell: 'powershell: && 仅 7+ 支持；命令是 cmdlet；尾部 & 语义不同',
};

const SHELL_BIN: Record<ShellType, string> = {
	zsh: '/bin/zsh',
	bash: '/bin/bash',
	fish: '/usr/local/bin/fish',
	cmd: 'cmd.exe',
	powershell: 'powershell.exe',
};

// 纠偏器：返回拒绝原因（null = 通过）；只写 zsh / bash 规则，其余占位
export function checkMisuse(input: ExecInput): string | null {
	const trimmed = input.command.trim();
	if (/&$/.test(trimmed) && !/&&$|>&$|&>$/.test(trimmed)) {
		return '命令以 & 结尾会被拒绝：请改用 background: true 参数运行后台任务，而不是 shell 后台符';
	}
	if (
		/\bcat\s+[^\s|>;]+$/u.test(trimmed) &&
		!/\.(log|json|txt|md|ts|js|rs|py|go|toml|yml|yaml)$/.test(trimmed)
	) {
		return '读单个文件请使用 read 工具而不是 cat（保留行号与 read-before-edit 追踪）';
	}
	if (
		input.type === 'zsh' &&
		/\$\{?[a-zA-Z_][a-zA-Z0-9_]*\[0\]/u.test(input.command)
	) {
		return 'zsh 数组下标从 1 开始，[0] 不是首个元素';
	}
	if (input.type === 'cmd' && /&&/.test(input.command)) {
		return 'cmd 不支持 &&，请用 & 串联或拆成多次 exec 调用';
	}
	return null;
}

function truncateFirstLast(
	text: string,
	maxBytes: number,
): { text: string; truncated: boolean } {
	if (text.length <= maxBytes) return { text, truncated: false };
	const half = Math.floor(maxBytes / 2);
	return {
		text: `${text.slice(0, half)}\n... [truncated ${text.length - maxBytes} chars] ...\n${text.slice(-half)}`,
		truncated: true,
	};
}

export async function exec(input: ExecInput): Promise<ExecResult> {
	const misuse = checkMisuse(input);
	if (misuse) {
		throw new Error(`命令被拒（纠偏）: ${misuse}`);
	}
	const bin = SHELL_BIN[input.type];
	const proc = Bun.spawn([bin, '-c', input.command], {
		cwd: resolve(input.path),
		stdout: 'pipe',
		stderr: 'pipe',
	});
	const timeout = input.timeout ?? DEFAULT_TIMEOUT_MS;
	const timer = setTimeout(() => {
		try {
			proc.kill('SIGTERM');
			setTimeout(() => proc.kill('SIGKILL'), 1000).unref();
		} catch {
			// 进程已退出
		}
	}, timeout);

	const [stdoutBuf, stderrBuf] = await Promise.all([
		new Response(proc.stdout).arrayBuffer(),
		new Response(proc.stderr).arrayBuffer(),
	]);
	const exitCode = await proc.exited;
	clearTimeout(timer);

	const combined = `${Buffer.from(stdoutBuf).toString()}${Buffer.from(stderrBuf).toString()}`;
	const { text, truncated } = truncateFirstLast(combined, MAX_OUTPUT_BYTES);

	let outputFile: string | undefined;
	if (truncated) {
		const dir = join(homedir(), '.kxen', 'exec-output');
		mkdirSync(dir, { recursive: true });
		outputFile = join(
			dir,
			`${Date.now()}-${Math.random().toString(36).slice(2, 8)}.log`,
		);
		writeFileSync(outputFile, combined);
	}

	return { exitCode, stdout: text, truncated, outputFile };
}
