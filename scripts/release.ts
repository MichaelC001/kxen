// release 管线：构建五平台二进制 + sha256，可选创建 GitHub Release
// 用法: bun run scripts/release.ts <version> [--publish]
import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const version = Bun.argv[2];
if (!version) {
	console.error('用法: bun run scripts/release.ts <version> [--publish]');
	process.exit(1);
}
const publish = Bun.argv.includes('--publish');

// 1. 构建矩阵
const build = Bun.spawn(['bun', 'run', 'scripts/build.ts', '--all'], {
	stdout: 'inherit',
	stderr: 'inherit',
});
if ((await build.exited) !== 0) throw new Error('构建失败');

// 2. sha256
const dist = join(process.cwd(), 'dist');
for (const file of readdirSync(dist)) {
	if (!file.startsWith('kxen')) continue;
	const data = readFileSync(join(dist, file));
	const sum = createHash('sha256').update(data).digest('hex');
	await Bun.write(join(dist, `${file}.sha256`), `${sum}  ${file}\n`);
	console.log(`${file}  ${sum.slice(0, 12)}...`);
}

// 3. 可选发布
if (publish) {
	const assets = readdirSync(dist)
		.filter((f) => f.startsWith('kxen'))
		.map((f) => join(dist, f));
	const gh = Bun.spawn(
		[
			'gh',
			'release',
			'create',
			version,
			'--title',
			version,
			'--notes',
			`kxen ${version}`,
			...assets,
		],
		{ stdout: 'inherit', stderr: 'inherit' },
	);
	if ((await gh.exited) !== 0) throw new Error('gh release create 失败');
	console.log(`已发布 ${version}`);
} else {
	console.log('构建完成（未发布，加 --publish 创建 release）');
}
