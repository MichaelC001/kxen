export interface HttpRequestOptions extends RequestInit {
	timeoutMs?: number;
}

// E4：所有出站请求统一走这里，保证 body 被完整消费或显式取消
export async function httpFetch(
	url: string,
	opts: HttpRequestOptions = {},
): Promise<Response> {
	const { timeoutMs = 60_000, ...init } = opts;
	const controller = new AbortController();
	const timer = setTimeout(() => controller.abort(), timeoutMs);
	try {
		return await fetch(url, { ...init, signal: controller.signal });
	} finally {
		clearTimeout(timer);
	}
}

// 强制排空并取消，防止 Response 缓冲滞留（Claude Code ArrayBuffer 泄漏的根因）
export async function drainResponse(res: Response): Promise<void> {
	try {
		if (res.body && !res.bodyUsed) await res.arrayBuffer();
	} catch {
		// 排空失败也要继续 cancel
	} finally {
		try {
			await res.body?.cancel();
		} catch {
			// 已关闭的流忽略
		}
	}
}

export async function readText(res: Response): Promise<string> {
	try {
		return await res.text();
	} finally {
		await drainResponse(res);
	}
}

export async function readJson<T = unknown>(res: Response): Promise<T> {
	try {
		return (await res.json()) as T;
	} finally {
		await drainResponse(res);
	}
}
