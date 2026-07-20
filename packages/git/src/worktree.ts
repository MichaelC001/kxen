import { resolve } from 'node:path';
import { git, statusPorcelain } from './git';

export interface WorktreeInfo {
	path: string;
	branch: string;
}

// git worktree 隔离：子代理 / workflow / 检查点在独立检出中工作
export async function createWorktree(
	repoRoot: string,
	name: string,
	baseRef = 'HEAD',
): Promise<WorktreeInfo> {
	const branch = `kxen/wt/${name}-${Date.now()}`;
	const path = resolve(repoRoot, '.kxen', 'worktrees', name);
	const r = await git(
		['worktree', 'add', '-b', branch, path, baseRef],
		repoRoot,
	);
	if (r.code !== 0) throw new Error(`git worktree add 失败: ${r.err}`);
	return { path, branch };
}

export async function removeWorktree(
	repoRoot: string,
	info: WorktreeInfo,
): Promise<void> {
	await git(['worktree', 'remove', '--force', info.path], repoRoot);
	await git(['branch', '-D', info.branch], repoRoot);
}

export async function listWorktrees(repoRoot: string): Promise<string[]> {
	const r = await git(['worktree', 'list', '--porcelain'], repoRoot);
	if (r.code !== 0) return [];
	return r.out
		.split('\n')
		.filter((line) => line.startsWith('worktree '))
		.map((line) => line.slice('worktree '.length));
}

export async function collectChangedFiles(
	worktreePath: string,
): Promise<string[]> {
	const entries = await statusPorcelain(worktreePath);
	return entries.map((e) => e.path);
}
