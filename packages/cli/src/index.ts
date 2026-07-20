#!/usr/bin/env bun

// KXEN_VERSION 由 scripts/build.ts 在编译期 define 注入，源码直跑时回落到包版本
const version = process.env.KXEN_VERSION ?? '0.0.0';

const [, , ...args] = Bun.argv;
const command = args[0];

switch (command) {
	case undefined: {
		const { runRepl } = await import('./repl');
		await runRepl({ cwd: process.cwd() });
		break;
	}
	case '-p': {
		const prompt = args[1];
		if (!prompt) {
			console.error('用法: kxen -p "<prompt>"');
			process.exitCode = 1;
			break;
		}
		const { runRepl } = await import('./repl');
		await runRepl({ cwd: process.cwd(), oneShot: prompt });
		break;
	}
	case 'help':
	case '--help':
	case '-h': {
		console.log(`kxen ${version}
终端 Coding Agent Harness

用法: kxen [command]

Commands:
  (无参数)   进入交互模式
  -p "<p>"   单发模式
  version    打印版本
  doctor     环境自检
`);
		break;
	}
	case 'version':
	case '--version':
	case '-v': {
		console.log(version);
		break;
	}
	case 'doctor': {
		console.log(`kxen ${version}`);
		console.log(`bun ${Bun.version}`);
		console.log(`platform ${process.platform}/${process.arch}`);
		break;
	}
	default: {
		console.error(`未知命令: ${command}`);
		process.exitCode = 1;
	}
}

export {};
