export interface StreamRule {
	id: string;
	pattern: RegExp;
	reminder: string;
	enabled: boolean;
}

export interface StreamRuleHit {
	rule: StreamRule;
	matched: string;
}

// TTSR（OMP time-travel stream rules 同型）：正则命中流式输出即触发纠正提醒
export class StreamRuleEngine {
	private rules: StreamRule[] = [];

	addRule(rule: StreamRule): void {
		this.rules.push(rule);
	}

	removeRule(id: string): void {
		this.rules = this.rules.filter((r) => r.id !== id);
	}

	list(): readonly StreamRule[] {
		return this.rules;
	}

	// 检查一段流式输出；命中返回规则与匹配文本（调用方负责中断流 + 注入 reminder + 重试）
	check(chunk: string): StreamRuleHit | undefined {
		for (const rule of this.rules) {
			if (!rule.enabled) continue;
			const match = rule.pattern.exec(chunk);
			if (match) return { rule, matched: match[0] };
		}
		return undefined;
	}
}
