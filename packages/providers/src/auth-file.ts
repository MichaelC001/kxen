import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';

// kxen auth.json 与 pi 同构：Record<providerId, Credential>
export type Credential =
	| { type: 'api_key'; key?: string }
	| ({
			type: 'oauth';
			refresh: string;
			access: string;
			expires: number;
	  } & Record<string, unknown>);

export function readAuthFile(authPath: string): Record<string, Credential> {
	if (!existsSync(authPath)) return {};
	try {
		return JSON.parse(readFileSync(authPath, 'utf8')) as Record<
			string,
			Credential
		>;
	} catch {
		return {};
	}
}

export function readCredential(
	authPath: string,
	providerId: string,
): Credential | undefined {
	return readAuthFile(authPath)[providerId];
}

export function writeCredential(
	authPath: string,
	providerId: string,
	cred: Credential,
): void {
	const data = readAuthFile(authPath);
	data[providerId] = cred;
	mkdirSync(dirname(authPath), { recursive: true });
	writeFileSync(authPath, JSON.stringify(data, null, 2));
}
