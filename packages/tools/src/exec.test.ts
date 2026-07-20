import { describe, expect, test } from 'bun:test';
import { checkMisuse, exec } from './exec';

describe('exec 纠偏器', () => {
	test('尾部 & 拒绝', () => {
		expect(
			checkMisuse({ type: 'bash', path: '.', command: 'npm run dev &' }),
		).toContain('background');
	});

	test('&& 结尾不算后台符', () => {
		expect(
			checkMisuse({ type: 'bash', path: '.', command: 'true &&' }),
		).toBeNull();
	});

	test('zsh 数组 [0] 纠偏', () => {
		expect(
			checkMisuse({ type: 'zsh', path: '.', command: 'echo ${arr[0]}' }),
		).toContain('zsh');
	});

	test('cmd 里用 && 拒绝', () => {
		expect(
			checkMisuse({ type: 'cmd', path: '.', command: 'dir && echo ok' }),
		).toContain('cmd');
	});

	test('正常命令通过', () => {
		expect(
			checkMisuse({
				type: 'bash',
				path: '.',
				command: 'git status && git log -1',
			}),
		).toBeNull();
	});
});

describe('exec 执行', () => {
	test('基本命令与 exitCode', async () => {
		const r = await exec({ type: 'bash', path: '/tmp', command: 'echo hello' });
		expect(r.exitCode).toBe(0);
		expect(r.stdout).toContain('hello');
		expect(r.truncated).toBe(false);
	});

	test('非零退出码', async () => {
		const r = await exec({ type: 'bash', path: '/tmp', command: 'exit 3' });
		expect(r.exitCode).toBe(3);
	});

	test('纠偏拒绝不执行', async () => {
		await expect(
			exec({ type: 'bash', path: '/tmp', command: 'sleep 10 &' }),
		).rejects.toThrow('纠偏');
	});
});
