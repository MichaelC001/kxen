// 跨平台二进制构建：bun build --compile 矩阵
// 单发: bun run scripts/build.ts ；全平台: bun run scripts/build.ts --all
import pkg from '../package.json';

const version: string = pkg.version;

const targets = [
	['darwin-arm64', 'kxen-darwin-arm64'],
	['darwin-x64', 'kxen-darwin-x64'],
	['linux-x64', 'kxen-linux-x64'],
	['linux-arm64', 'kxen-linux-arm64'],
	['windows-x64', 'kxen-windows-x64.exe'],
] as const;

const all = Bun.argv.includes('--all');

async function compile(args: string[], outfile: string): Promise<void> {
	const proc = Bun.spawn(
		[
			'bun',
			'build',
			'packages/cli/src/index.ts',
			'--compile',
			'--minify',
			'--define',
			`process.env.KXEN_VERSION="${version}"`,
			...args,
			`--outfile=${outfile}`,
		],
		{ stdout: 'inherit', stderr: 'inherit' },
	);
	const code = await proc.exited;
	if (code !== 0) {
		console.error(`构建失败: ${outfile} (exit ${code})`);
		process.exit(1);
	}
	console.log(outfile);
}

if (all) {
	for (const [target, name] of targets) {
		await compile([`--target=bun-${target}`], `dist/${name}`);
	}
} else {
	await compile([], 'dist/kxen');
}
