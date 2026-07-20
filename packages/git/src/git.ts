export interface GitResult {
	code: number;
	out: string;
	err: string;
}

// 统一 git 调用入口：禁用分页与颜色，输出稳定可解析
export async function git(args: string[], cwd: string): Promise<GitResult> {
	const proc = Bun.spawn(['git', '--no-pager', '--color=never', ...args], {
		cwd,
		stdout: 'pipe',
		stderr: 'pipe',
	});
	const [out, err, code] = await Promise.all([
		new Response(proc.stdout).text(),
		new Response(proc.stderr).text(),
		proc.exited,
	]);
	return { code, out: out.trim(), err: err.trim() };
}

export async function gitRoot(cwd: string): Promise<string | undefined> {
	const r = await git(['rev-parse', '--show-toplevel'], cwd);
	return r.code === 0 ? r.out : undefined;
}

export async function currentBranch(cwd: string): Promise<string | undefined> {
	const r = await git(['branch', '--show-current'], cwd);
	return r.code === 0 && r.out ? r.out : undefined;
}

export interface StatusEntry {
	xy: string;
	path: string;
}

export async function statusPorcelain(cwd: string): Promise<StatusEntry[]> {
	const r = await git(['status', '--porcelain'], cwd);
	if (r.code !== 0 || !r.out) return [];
	return r.out
		.split('\n')
		.map((line) => ({ xy: line.slice(0, 2), path: line.slice(3).trim() }));
}

export async function headCommit(cwd: string): Promise<string | undefined> {
	const r = await git(['rev-parse', 'HEAD'], cwd);
	return r.code === 0 ? r.out : undefined;
}

export async function gitLog(cwd: string, n: number): Promise<string[]> {
	const r = await git(['log', `-${n}`, '--oneline'], cwd);
	if (r.code !== 0 || !r.out) return [];
	return r.out.split('\n');
}
