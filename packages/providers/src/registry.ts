import type { Credential } from './auth-file';

export interface AuthContext {
	authPath: string;
	env: NodeJS.ProcessEnv;
	// 测试注入用：覆盖各官方 CLI 的凭证文件路径
	cliAuthPaths?: Record<string, string>;
}

export interface SmokeResult {
	ok: boolean;
	detail: string;
}

// 注册表式 provider 认证：新增一个主流 provider = 新增一个实现文件并注册
export interface ProviderAuth {
	id: string;
	displayName: string;
	// 解析凭证：kxen auth.json -> 官方 CLI 现有凭证导入 -> 环境变量
	resolve(ctx: AuthContext): Promise<Credential | undefined>;
	// 真实调用冒烟（M1 末验证用）
	smoke?(cred: Credential): Promise<SmokeResult>;
}

const registry = new Map<string, ProviderAuth>();

export function registerProviderAuth(auth: ProviderAuth): void {
	registry.set(auth.id, auth);
}

export function getProviderAuth(id: string): ProviderAuth | undefined {
	return registry.get(id);
}

export function listProviderAuths(): ProviderAuth[] {
	return [...registry.values()];
}
