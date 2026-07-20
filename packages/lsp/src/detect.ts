import { existsSync } from 'node:fs';
import { join } from 'node:path';

export interface LspServerSpec {
	languageId: string;
	command: string;
	args: string[];
	extensions: string[];
	markers: string[];
}

// 内置映射表（analysis/09 L2）：标记文件 -> 语言 -> LS 二进制
export const BUILTIN_SERVERS: LspServerSpec[] = [
	{
		languageId: 'typescript',
		command: 'typescript-language-server',
		args: ['--stdio'],
		extensions: ['.ts', '.tsx', '.mts', '.cts'],
		markers: ['tsconfig.json', 'package.json'],
	},
	{
		languageId: 'javascript',
		command: 'typescript-language-server',
		args: ['--stdio'],
		extensions: ['.js', '.jsx', '.mjs', '.cjs'],
		markers: ['package.json'],
	},
	{
		languageId: 'go',
		command: 'gopls',
		args: [],
		extensions: ['.go'],
		markers: ['go.mod'],
	},
	{
		languageId: 'rust',
		command: 'rust-analyzer',
		args: [],
		extensions: ['.rs'],
		markers: ['Cargo.toml'],
	},
	{
		languageId: 'python',
		command: 'pyright-langserver',
		args: ['--stdio'],
		extensions: ['.py'],
		markers: ['pyproject.toml', 'requirements.txt', 'setup.py'],
	},
];

// 扫描项目标记文件推断语言集
export function detectLanguages(root: string): string[] {
	const found = new Set<string>();
	for (const spec of BUILTIN_SERVERS) {
		if (spec.markers.some((m) => existsSync(join(root, m)))) {
			found.add(spec.languageId);
		}
	}
	return [...found];
}

// 探测 PATH 上的 server 二进制
export async function findServerBinary(
	command: string,
): Promise<string | undefined> {
	return Bun.which(command) ?? undefined;
}

export interface ResolvedServer extends LspServerSpec {
	binaryPath: string;
}

// auto-detect：项目语言 -> PATH 二进制 -> 懒启动清单
export async function detectServers(root: string): Promise<ResolvedServer[]> {
	const languages = detectLanguages(root);
	const out: ResolvedServer[] = [];
	for (const spec of BUILTIN_SERVERS) {
		if (!languages.includes(spec.languageId)) continue;
		const binaryPath = await findServerBinary(spec.command);
		if (binaryPath) out.push({ ...spec, binaryPath });
	}
	return out;
}
