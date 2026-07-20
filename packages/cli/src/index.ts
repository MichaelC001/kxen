#!/usr/bin/env bun

import { homedir } from 'node:os';
import { join } from 'node:path';
import { parseArgs } from 'node:util';
import { kxenExtension } from './extensions/kxen';

// KXEN_VERSION 由 scripts/build.ts 在编译期 define 注入
const version = process.env.KXEN_VERSION ?? '0.0.0';

const { values, positionals } = parseArgs({
	args: Bun.argv.slice(2),
	options: {
		help: { type: 'boolean', short: 'h' },
		version: { type: 'boolean', short: 'v' },
	},
	strict: false,
	allowPositionals: true,
});

async function main(): Promise<void> {
	if (values.help) {
		console.log(`kxen ${version}
终端 Coding Agent Harness（基于 pi 增强，pi 的全部 CLI 参数直接可用）

用法: kxen [pi 原生参数] [command]

kxen 特有命令:
  doctor    环境自检
  upgrade   自更新（GitHub Releases）

kxen 特有 slash 命令（会话内）:
  /write-goal   交互式创建 goal（收集 -> 确认 -> 自动执行）
  /goal         查看 / 执行 goal
  /workflow     运行 workflow 编排脚本

pi 原生参数示例:
  -p "<prompt>"            单发模式
  --model <provider/id>    指定模型
  --resume / --continue    恢复会话
  -e, --extension <path>   加载扩展
  --no-extensions 等       详见 pi 文档
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
		console.log(
			`agent dir: ${process.env.PI_CODING_AGENT_DIR ?? join(homedir(), '.kxen', 'agent')}`,
		);
		return;
	}

	if (positionals[0] === 'upgrade') {
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

	// 其余参数全部穿透给 pi 的 main()；agent dir 统一指到 kxen 目录
	process.env.PI_CODING_AGENT_DIR ??= join(homedir(), '.kxen', 'agent');
	const { main: piMain } = await import('@earendil-works/pi-coding-agent');
	await piMain(Bun.argv.slice(2), { extensionFactories: [kxenExtension] });
}

main().catch((err: unknown) => {
	console.error(err instanceof Error ? err.message : String(err));
	process.exit(1);
});
