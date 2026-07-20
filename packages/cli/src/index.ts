#!/usr/bin/env bun

import { parseArgs } from 'node:util';
import { runInteractive, runPrint } from './interactive';
import { runUpgrade } from './upgrade';

// KXEN_VERSION 由 scripts/build.ts 在编译期 define 注入，源码直跑时回落到包版本
const version = process.env.KXEN_VERSION ?? '0.0.0';

const { values, positionals } = parseArgs({
	args: Bun.argv.slice(2),
	options: {
		print: { type: 'string', short: 'p' },
		model: { type: 'string' },
		plan: { type: 'boolean' },
		'disable-plan': { type: 'boolean' },
		yolo: { type: 'boolean', short: 'y' },
		'append-system-prompt': { type: 'string' },
		help: { type: 'boolean', short: 'h' },
		version: { type: 'boolean', short: 'v' },
	},
	strict: true,
	allowPositionals: true,
});

async function main(): Promise<void> {
	if (values.help) {
		console.log(`kxen ${version}
终端 Coding Agent Harness

用法: kxen [flags] [command]

Flags:
  -p, --print "<prompt>"    单发模式
  --model <provider/model>  指定模型（如 anthropic/claude-fable-5）
  --plan                    以 plan 模式启动（只读）
  --disable-plan            禁用 plan 模式
  -y, --yolo                跳过所有确认（全自动）
  --append-system-prompt    追加系统提示词
  -v, --version             打印版本

Commands:
  doctor                    环境自检
  upgrade                   自更新（GitHub Releases）
`);
		return;
	}

	if (values.version) {
		console.log(version);
		return;
	}

	if (positionals[0] === 'doctor') {
		console.log(`kxen ${version}`);
		console.log(`bun ${Bun.version}`);
		console.log(`platform ${process.platform}/${process.arch}`);
		return;
	}

	if (positionals[0] === 'upgrade') {
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

	process.env.KXEN_PLAN = values.plan ? '1' : '';
	process.env.KXEN_DISABLE_PLAN = values['disable-plan'] ? '1' : '';
	process.env.KXEN_YOLO = values.yolo ? '1' : '';
	process.env.KXEN_MODEL = values.model ?? '';
	process.env.KXEN_APPEND_SYSTEM_PROMPT = values['append-system-prompt'] ?? '';

	if (values.print !== undefined) {
		const code = await runPrint(process.cwd(), values.print);
		process.exitCode = code;
		return;
	}

	await runInteractive(process.cwd());
}

main().catch((err: unknown) => {
	console.error(err instanceof Error ? err.message : String(err));
	process.exit(1);
});
