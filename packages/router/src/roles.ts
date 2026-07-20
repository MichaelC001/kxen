import { existsSync, readFileSync } from 'node:fs';

export interface RoleSpec {
	model: string;
	fallbacks?: string[];
}

export type RolesConfig = Record<string, RoleSpec>;

export const BUILTIN_ROLES = [
	'thinking',
	'planning',
	'execution',
	'review',
	'research',
	'tiny',
] as const;

export interface ResolvedModel {
	model: string;
	thinkingLevel?: string;
}

// "anthropic/claude-opus:high" -> { model: "anthropic/claude-opus", thinkingLevel: "high" }
export function parseThinkingSuffix(spec: string): ResolvedModel {
	const idx = spec.lastIndexOf(':');
	if (idx > 0 && !spec.includes(':/')) {
		const level = spec.slice(idx + 1);
		if (/^[a-z]+$/.test(level)) {
			return { model: spec.slice(0, idx), thinkingLevel: level };
		}
	}
	return { model: spec };
}

export function loadRoles(path: string): RolesConfig {
	if (!existsSync(path)) return {};
	const parsed = Bun.TOML.parse(readFileSync(path, 'utf8')) as {
		roles?: RolesConfig;
	};
	return parsed.roles ?? {};
}

export interface ResolvedRole {
	primary: ResolvedModel;
	chain: ResolvedModel[];
}

// 角色 -> 首选 + fallback 链（含 thinking 后缀解析）
export function resolveRole(
	roles: RolesConfig,
	role: string,
): ResolvedRole | undefined {
	const spec = roles[role];
	if (!spec) return undefined;
	return {
		primary: parseThinkingSuffix(spec.model),
		chain: (spec.fallbacks ?? []).map(parseThinkingSuffix),
	};
}

export function providerOf(model: string): string {
	const idx = model.indexOf('/');
	return idx === -1 ? model : model.slice(0, idx);
}
