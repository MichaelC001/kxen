export interface PromptSection {
	id: string;
	// 数值越小越靠前；静态段必须在前保证缓存前缀稳定
	priority: number;
	content: string;
	// cache: 该段跨轮字节级不变
	cache?: boolean;
}

// 模板变量插值：{{tools.by_kind.X}} 风格（P2，禁硬编码工具名）
export function renderTemplate(
	template: string,
	vars: Record<string, string>,
): string {
	return template.replace(
		/\{\{\s*([a-zA-Z0-9_.-]+)\s*\}\}/g,
		(match, key: string) => vars[key] ?? match,
	);
}

// 条件段：{{#if key}}...{{/if}}，key 存在且非空才保留
export function renderConditionals(
	template: string,
	present: Record<string, boolean>,
): string {
	return template.replace(
		/\{\{#if\s+([a-zA-Z0-9_.-]+)\}\}([\s\S]*?)\{\{\/if\}\}/g,
		(_m, key: string, body: string) => (present[key] ? body : ''),
	);
}

// section-based composer（P1）：排序后拼接，静态段在前
export function composePrompt(sections: PromptSection[]): string {
	return [...sections]
		.sort((a, b) => a.priority - b.priority)
		.map((s) => s.content.trim())
		.filter(Boolean)
		.join('\n\n');
}

// 迟绑定注入（P7）：请求末尾追加，不进静态前缀
export function injectLateBinding(
	entries: Array<{ role: string; content: string }>,
	prompt: string,
): void {
	entries.push({ role: 'user', content: prompt });
}
