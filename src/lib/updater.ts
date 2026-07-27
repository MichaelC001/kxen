import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";

export type AvailableUpdate = NonNullable<Awaited<ReturnType<typeof check>>>;

export async function currentVersion(): Promise<string> {
  return getVersion();
}

export async function checkForUpdate(): Promise<AvailableUpdate | null> {
  return check();
}

export async function installUpdate(update: AvailableUpdate): Promise<void> {
  await update.downloadAndInstall();
  await relaunch();
}
