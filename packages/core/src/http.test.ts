import { describe, expect, test } from 'bun:test';
import { drainResponse, httpFetch, readJson } from './http';

describe('http', () => {
	test('httpFetch 超时触发 abort', async () => {
		const server = Bun.serve({
			port: 0,
			fetch: () => new Promise(() => {}),
		});
		try {
			const start = Date.now();
			await expect(
				httpFetch(`http://localhost:${server.port}/`, { timeoutMs: 50 }),
			).rejects.toThrow();
			expect(Date.now() - start).toBeLessThan(3000);
		} finally {
			server.stop(true);
		}
	});

	test('readJson 返回数据并排空', async () => {
		const server = Bun.serve({
			port: 0,
			fetch: () => Response.json({ ok: true }),
		});
		try {
			const res = await httpFetch(`http://localhost:${server.port}/`);
			const data = await readJson<{ ok: boolean }>(res);
			expect(data.ok).toBe(true);
		} finally {
			server.stop(true);
		}
	});

	test('drainResponse 对未消费 body 不抛错', async () => {
		const server = Bun.serve({
			port: 0,
			fetch: () => new Response('x'.repeat(1024)),
		});
		try {
			const res = await httpFetch(`http://localhost:${server.port}/`);
			await drainResponse(res);
		} finally {
			server.stop(true);
		}
	});
});
