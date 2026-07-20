import type { MRMStatus } from '@kxen/resources';

export interface StatuslineInput {
	model?: string;
	mode?: string;
	mrm?: MRMStatus;
	contextUsedPct?: number;
	budgetWatermark?: number;
	cwd?: string;
}

function lamp(ok: boolean | undefined): string {
	if (ok === undefined) return 'OFF';
	return ok ? 'PASS' : 'FAIL';
}

// 默认状态行：模型 / 模式 / MRM 各 provider 灯 / context 水位 / 预算水位
export function renderStatusline(input: StatuslineInput): string {
	const parts: string[] = [];
	parts.push(input.model ?? 'no-model');
	parts.push(input.mode ?? 'build');
	if (input.mrm) {
		const providers = Object.entries(input.mrm.providers)
			.map(([id, p]) => `${id}:${lamp(!p.coolingDown)}${p.inFlight}/${p.max}`)
			.join(' ');
		parts.push(
			`mrm[${input.mrm.global.inFlight}/${input.mrm.global.max} ${providers}]`,
		);
	}
	if (input.contextUsedPct !== undefined) {
		parts.push(`ctx ${Math.round(input.contextUsedPct)}%`);
	}
	if (input.budgetWatermark !== undefined && input.budgetWatermark > 0) {
		parts.push(`budget ${Math.round(input.budgetWatermark * 100)}%`);
	}
	if (input.cwd) parts.push(input.cwd);
	return parts.join(' | ');
}
