import { describe, expect, test } from 'bun:test';
import { EventBus } from '@kxen/core';
import { HookRegistry, pickDecision, type RegisteredHook } from './hooks';

describe('hooks', () => {
	test('pickDecision 优先级 deny > defer > ask > allow', () => {
		expect(pickDecision(['allow', 'ask'])).toBe('ask');
		expect(pickDecision(['allow', 'deny', 'defer'])).toBe('deny');
		expect(pickDecision([])).toBeUndefined();
	});

	test('builtin hook 执行与 matcher 过滤', async () => {
		const bus = new EventBus();
		const registry = new HookRegistry(bus);
		let fired = 0;
		registry.register({
			id: 'h1',
			event: 'PreToolUse',
			matcher: 'exec|write',
			hook: {
				kind: 'builtin',
				handler: () => {
					fired++;
					return { decision: 'allow' };
				},
			},
			enabled: true,
			source: 'builtin',
		});
		await registry.run('PreToolUse', { toolName: 'exec', cwd: '/tmp' });
		await registry.run('PreToolUse', { toolName: 'read', cwd: '/tmp' });
		expect(fired).toBe(1);
	});

	test('disabled hook 不触发', async () => {
		const bus = new EventBus();
		const registry = new HookRegistry(bus);
		let fired = 0;
		registry.register({
			id: 'h1',
			event: 'PreToolUse',
			hook: {
				kind: 'builtin',
				handler: () => {
					fired++;
				},
			},
			enabled: false,
			source: 'global',
		});
		await registry.run('PreToolUse', { cwd: '/tmp' });
		expect(fired).toBe(0);
	});

	test('command hook exit 2 阻断并带原因', async () => {
		const bus = new EventBus();
		const registry = new HookRegistry(bus);
		registry.register({
			id: 'h1',
			event: 'PreToolUse',
			hook: { kind: 'command', command: 'echo 危险 >&2; exit 2' },
			enabled: true,
			source: 'project',
		});
		const outputs = await registry.run('PreToolUse', { cwd: '/tmp' });
		expect(outputs[0]?.decision).toBe('deny');
		expect(outputs[0]?.reason).toBe('危险');
	});

	test('command hook exit 0 + JSON 输出', async () => {
		const bus = new EventBus();
		const registry = new HookRegistry(bus);
		registry.register({
			id: 'h1',
			event: 'PreToolUse',
			hook: { kind: 'command', command: 'echo \'{"decision":"ask"}\'' },
			enabled: true,
			source: 'project',
		});
		const outputs = await registry.run('PreToolUse', { cwd: '/tmp' });
		expect(outputs[0]?.decision).toBe('ask');
	});
});
