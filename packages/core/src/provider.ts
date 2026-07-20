import { homedir } from 'node:os';
import { join } from 'node:path';

export const KXEN_AGENT_DIR =
	process.env.KXEN_AGENT_DIR ?? join(homedir(), '.kxen', 'agent');
export const KXEN_AUTH_PATH = join(KXEN_AGENT_DIR, 'auth.json');

// Bun.write 自动创建父目录，无需 mkdir；slash 命令走 inline extension（pi main 注入），不写模板文件
export async function ensureAgentDir(): Promise<string> {
	await Bun.write(join(KXEN_AGENT_DIR, '.keep'), '');
	return KXEN_AGENT_DIR;
}
