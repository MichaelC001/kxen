// E2E: 多订阅混用验证——execution 走 xai，review 走 kimi，验证两个 provider 都被真实调用

import { BUILTIN_SUBAGENTS } from '@kxen/subagent';
import { buildStack } from './lib/stack';

const cwd = process.cwd();
const { manager } = await buildStack(cwd);

const executeSpec = BUILTIN_SUBAGENTS.find((s) => s.name === 'execute');
const reviewSpec = BUILTIN_SUBAGENTS.find((s) => s.name === 'review');
if (!executeSpec || !reviewSpec) throw new Error('内置 spec 缺失');

console.log(
	'派发 execution（应路由到 xai）与 review（应路由到 kimi-coding）...',
);
const [execHandle, reviewHandle] = await Promise.all([
	manager.spawn(executeSpec, '用一句话回答：1+1 等于几？只回答数字。'),
	manager.spawn(reviewSpec, '用一句话回答：2+2 等于几？只回答数字。'),
]);
const [execResult, reviewResult] = await Promise.all([
	execHandle.result,
	reviewHandle.result,
]);

console.log(
	`execution -> provider=${execResult.provider} model=${execResult.model} summary=${execResult.summary.slice(0, 50)}`,
);
console.log(
	`review    -> provider=${reviewResult.provider} model=${reviewResult.model} summary=${reviewResult.summary.slice(0, 50)}`,
);

const providers = new Set(
	[execResult.provider, reviewResult.provider].filter(Boolean),
);
const ok =
	execResult.stopReason === 'completed' &&
	reviewResult.stopReason === 'completed' &&
	providers.size >= 2;
console.log(ok ? 'PASS 多订阅混用验证（两个 provider 均被真实调用）' : 'FAIL');
process.exit(ok ? 0 : 1);
