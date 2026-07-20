#!/usr/bin/env bun

import { parseArgs } from 'node:util';

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
`);
	process.exit(0);
}

if (values.version) {
	console.log(version);
	process.exit(0);
}

if (positionals[0] === 'doctor') {
	console.log(`kxen ${version}`);
	console.log(`bun ${Bun.version}`);
	console.log(`platform ${process.platform}/${process.arch}`);
	process.exit(0);
}

if (positionals[0] === 'upgrade') {
	const { runUpgrade } = await import('./upgrade');
	try {
		await runUpgrade();
	} catch (err) {
		console.error(`升级失败: ${err instanceof Error ? err.message : String(err)}`);
		process.exitCode = 1;
	}
	process.exit(process.exitCode ?? 0);
}

process.env.KXEN_PLAN = values.plan ? '1' : '';
process.env.KXEN_DISABLE_PLAN = values['disable-plan'] ? '1' : '';
process.env.KXEN_YOLO = values.yolo ? '1' : '';
process.env.KXEN_MODEL = values.model ?? '';
process.env.KXEN_APPEND_SYSTEM_PROMPT = values['append-system-prompt'] ?? '';

if (values.print !== undefined) {
	const { runPrint } = await import('./interactive');
	const code = await runPrint(process.cwd(), values.print);
	process.exitCode = code;
} else {
	const { runInteractive } = await import('./interactive');
	await runInteractive(process.cwd());
}
