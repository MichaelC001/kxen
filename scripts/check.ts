// 一键检查：biome -> typecheck -> test，任一失败即停
const steps: [string, string[]][] = [
	['biome check', ['bunx', 'biome', 'check', '.']],
	['typecheck', ['bunx', 'tsc', '-p', 'tsconfig.base.json', '--noEmit']],
	['bun test', ['bun', 'test']],
];

for (const [name, cmd] of steps) {
	console.log(`== ${name} ==`);
	const code = await Bun.spawn(cmd, { stdout: 'inherit', stderr: 'inherit' })
		.exited;
	if (code !== 0) {
		console.error(`${name} 失败 (exit ${code})`);
		process.exit(code);
	}
}
console.log('全部通过');

export {};
