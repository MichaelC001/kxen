import { createHash } from 'node:crypto';
import { chmodSync, copyFileSync, existsSync, renameSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const REPO = 'StringKe/kxen';

function platformAssetName(): string {
	const platform =
		process.platform === 'darwin'
			? 'darwin'
			: process.platform === 'win32'
				? 'windows'
				: 'linux';
	const arch = process.arch === 'arm64' ? 'arm64' : 'x64';
	return `kxen-${platform}-${arch}${platform === 'windows' ? '.exe' : ''}`;
}

async function sha256File(path: string): Promise<string> {
	const data = await Bun.file(path).arrayBuffer();
	return createHash('sha256').update(Buffer.from(data)).digest('hex');
}

// 自更新：GitHub Releases 拉最新版 -> sha256 校验 -> 原子替换当前二进制
export async function runUpgrade(): Promise<void> {
	const current = process.env.KXEN_VERSION ?? '0.0.0';
	console.log(`当前版本: ${current}`);

	const meta = (await (
		await fetch(`https://api.github.com/repos/${REPO}/releases/latest`)
	).json()) as {
		tag_name?: string;
		assets?: { name: string; browser_download_url: string }[];
	};
	if (!meta.tag_name)
		throw new Error('未找到 release（kxen 还没有发布过版本）');
	console.log(`最新版本: ${meta.tag_name}`);
	if (meta.tag_name === current || meta.tag_name === `v${current}`) {
		console.log('已是最新');
		return;
	}

	const assetName = platformAssetName();
	const asset = meta.assets?.find((a) => a.name === assetName);
	const checksum = meta.assets?.find((a) => a.name === `${assetName}.sha256`);
	if (!asset) throw new Error(`release 中没有本平台产物: ${assetName}`);

	const tmp = join(tmpdir(), `kxen-upgrade-${Date.now()}`);
	console.log(`下载 ${assetName} ...`);
	const binRes = await fetch(asset.browser_download_url);
	if (!binRes.ok) throw new Error(`下载失败: HTTP ${binRes.status}`);
	await Bun.write(tmp, await binRes.arrayBuffer());

	if (checksum) {
		const text = await (await fetch(checksum.browser_download_url)).text();
		const expected = text.trim().split(/\s+/)[0] ?? '';
		const actual = await sha256File(tmp);
		if (expected && actual !== expected)
			throw new Error(`sha256 校验失败: 期望 ${expected} 实际 ${actual}`);
		console.log('sha256 校验通过');
	}

	const target = process.execPath;
	chmodSync(tmp, 0o755);
	if (!existsSync(target)) throw new Error(`无法定位当前二进制: ${target}`);
	// 原子替换：先换名旧的，再移入新的，失败可回滚
	const backup = `${target}.bak-${Date.now()}`;
	renameSync(target, backup);
	try {
		renameSync(tmp, target);
	} catch (err) {
		copyFileSync(backup, target);
		throw err;
	}
	console.log(`已升级到 ${meta.tag_name}（旧版备份: ${backup}）`);
}
