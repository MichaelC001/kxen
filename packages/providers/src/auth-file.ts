// kxen auth.json 与 pi 同构：Record<providerId, Credential>
export type Credential =
	| { type: 'api_key'; key?: string }
	| ({
			type: 'oauth';
			refresh: string;
			access: string;
			expires: number;
	  } & Record<string, unknown>);

export async function readAuthFile(
	authPath: string,
): Promise<Record<string, Credential>> {
	const file = Bun.file(authPath);
	if (!(await file.exists())) return {};
	try {
		return (await file.json()) as Record<string, Credential>;
	} catch {
		return {};
	}
}

export async function readCredential(
	authPath: string,
	providerId: string,
): Promise<Credential | undefined> {
	return (await readAuthFile(authPath))[providerId];
}

export async function writeCredential(
	authPath: string,
	providerId: string,
	cred: Credential,
): Promise<void> {
	const data = await readAuthFile(authPath);
	data[providerId] = cred;
	await Bun.write(Bun.file(authPath), JSON.stringify(data, null, 2));
}
