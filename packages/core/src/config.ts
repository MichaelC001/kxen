import { existsSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

export interface RoleSpec {
	model: string;
	fallbacks?: string[];
}

export interface LimitsConfig {
	global?: { concurrent?: number };
	providers?: Record<string, { concurrent?: number; rpm?: number }>;
	roles?: Record<string, { concurrent?: number }>;
}

export interface BudgetsConfig {
	session?: { tokens?: number; costUsd?: number };
	perGoal?: { tokens?: number; agents?: number };
}

export interface KxenConfig {
	roles: Record<string, RoleSpec>;
	limits: LimitsConfig;
	budgets: BudgetsConfig;
	providers: Record<string, Record<string, unknown>>;
}

export interface LoadConfigOptions {
	globalPath?: string;
	projectDir?: string;
	overrides?: Partial<KxenConfig>;
}

const DEFAULTS: KxenConfig = {
	roles: {},
	limits: {},
	budgets: {},
	providers: {},
};

const isPlainObject = (v: unknown): v is Record<string, unknown> =>
	typeof v === 'object' && v !== null && !Array.isArray(v);

// 数组合并语义：整层替换（对齐 OMP settings），对象深合并
function merge<T>(base: T, override: unknown): T {
	if (override === undefined) return base;
	if (Array.isArray(override)) return override as T;
	if (isPlainObject(base) && isPlainObject(override)) {
		const out: Record<string, unknown> = { ...base };
		for (const [k, v] of Object.entries(override)) out[k] = merge(out[k], v);
		return out as T;
	}
	return (override === undefined ? base : override) as T;
}

function readToml(path: string): Record<string, unknown> {
	return Bun.TOML.parse(readFileSync(path, 'utf8')) as Record<string, unknown>;
}

export function loadConfig(opts: LoadConfigOptions = {}): KxenConfig {
	let cfg = merge(DEFAULTS, structuredClone(DEFAULTS));
	const globalPath = opts.globalPath ?? join(homedir(), '.kxen', 'config.toml');
	if (existsSync(globalPath)) cfg = merge(cfg, readToml(globalPath));
	if (opts.projectDir) {
		const projectPath = join(opts.projectDir, '.agents', 'config.toml');
		if (existsSync(projectPath)) cfg = merge(cfg, readToml(projectPath));
	}
	if (opts.overrides) cfg = merge(cfg, opts.overrides);
	return cfg;
}
