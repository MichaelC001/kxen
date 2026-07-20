import { mkdirSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

export const KXEN_AGENT_DIR = join(homedir(), '.kxen', 'agent');
export const KXEN_AUTH_PATH = join(KXEN_AGENT_DIR, 'auth.json');

export function ensureAgentDir(): string {
	mkdirSync(KXEN_AGENT_DIR, { recursive: true });
	return KXEN_AGENT_DIR;
}
