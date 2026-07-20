import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readdirSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { git } from '@kxen/git';

// shadow git 检查点（Gemini CLI 同型）：文件修改前快照到独立 git 仓库，可整体回滚
export class CheckpointStore {
	private shadowDir: string;

	constructor(projectRoot: string) {
		const hash = createHash('sha1')
			.update(projectRoot)
			.digest('hex')
			.slice(0, 12);
		this.shadowDir = join(homedir(), '.kxen', 'checkpoints', hash);
	}

	private env(): NodeJS.ProcessEnv {
		return {
			...process.env,
			GIT_DIR: join(this.shadowDir, 'repo.git'),
			GIT_WORK_TREE: undefined,
		};
	}

	private async shadow(args: string[], cwd: string) {
		const proc = Bun.spawn(
			[
				'git',
				'--git-dir',
				join(this.shadowDir, 'repo.git'),
				'--work-tree',
				cwd,
				...args,
			],
			{
				cwd,
				stdout: 'pipe',
				stderr: 'pipe',
			},
		);
		const [out, err, code] = await Promise.all([
			new Response(proc.stdout).text(),
			new Response(proc.stderr).text(),
			proc.exited,
		]);
		return { code, out: out.trim(), err: err.trim() };
	}

	async ensureRepo(cwd: string): Promise<void> {
		if (existsSync(join(this.shadowDir, 'repo.git'))) return;
		mkdirSync(join(this.shadowDir, 'repo.git'), { recursive: true });
		await this.shadow(['init'], cwd);
	}

	// 快照当前工作区（含未提交修改）；返回 commit id 作为检查点名
	async create(cwd: string, label: string): Promise<string> {
		await this.ensureRepo(cwd);
		await this.shadow(['add', '-A'], cwd);
		const r = await this.shadow(['commit', '-m', label, '--allow-empty'], cwd);
		if (r.code !== 0 && !r.err.includes('nothing to commit')) {
			throw new Error(`检查点提交失败: ${r.err}`);
		}
		const head = await this.shadow(['rev-parse', 'HEAD'], cwd);
		return head.out;
	}

	list(): string[] {
		const dir = join(this.shadowDir, 'repo.git');
		if (!existsSync(dir)) return [];
		return readdirSync(dir);
	}

	// 回滚到指定检查点（危险操作，调用方负责确认）
	async restore(cwd: string, commit: string): Promise<void> {
		const r = await this.shadow(['reset', '--hard', commit], cwd);
		if (r.code !== 0) throw new Error(`回滚失败: ${r.err}`);
	}
}
