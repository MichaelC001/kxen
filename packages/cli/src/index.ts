#!/usr/bin/env bun

import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { kxenExtension } from './extensions/kxen';

// KXEN_VERSION 由 scripts/build.ts 在编译期 define 注入
const version = process.env.KXEN_VERSION ?? '0.0.0';

// pi 官方换牌机制：piConfig（packages/cli/package.json）+ PI_PACKAGE_DIR
// 此后 APP_NAME=kxen、退出提示为 kxen --session、agent 目录变量为 KXEN_CODING_AGENT_DIR
const sourcePkgDir = dirname(dirname(fileURLToPath(import.meta.url)));
if (existsSync(join(sourcePkgDir, 'package.json'))) {
	process.env.PI_PACKAGE_DIR ??= sourcePkgDir;
}
process.env.KXEN_CODING_AGENT_DIR ??= join(homedir(), '.kxen', 'agent');

async function main(): Promise<void> {
	const [, , command, ...rest] = Bun.argv;

	if (command === 'version' || command === '--version' || command === '-v') {
		console.log(version);
		return;
	}

	if (command === 'doctor') {
		console.log(`kxen ${version}`);
		console.log(`bun ${Bun.version}`);
		console.log(`platform ${process.platform}/${process.arch}`);
		console.log(`agent dir: ${process.env.KXEN_CODING_AGENT_DIR}`);
		return;
	}

	if (command === 'upgrade') {
		const { runUpgrade } = await import('./upgrade');
		try {
			await runUpgrade();
		} catch (err) {
			console.error(
				`升级失败: ${err instanceof Error ? err.message : String(err)}`,
			);
			process.exitCode = 1;
		}
		return;
	}

	// 其余一切（含 --help、-p、--model、--resume 等）全部交给 pi 自己处理
	const { main: piMain } = await import('@earendil-works/pi-coding-agent');
	await piMain(Bun.argv.slice(2), { extensionFactories: [kxenExtension] });
}

main().catch((err: unknown) => {
	console.error(err instanceof Error ? err.message : String(err));
	process.exit(1);
});
