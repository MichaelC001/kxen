import { client } from "./client";

export interface VerifyOutcome {
  ok: boolean;
  latency_ms: number;
  detail: string;
}

export function providerVerify(provider: string, account?: string): Promise<VerifyOutcome> {
  return client.rpc("provider.verify", account ? { provider, account } : { provider });
}

export interface AccountInfo {
  provider: string;
  account: string;
  id: string;
  expired: boolean;
}

export function providerAccounts(): Promise<AccountInfo[]> {
  return client.rpc("provider.accounts");
}

export function importAccount(
  provider: string,
  account: string,
  access: string,
  refresh = "",
  expires = 0,
): Promise<void> {
  return client.rpc("provider.import_account", { provider, account, access, refresh, expires });
}

export function removeAccount(provider: string, account: string): Promise<void> {
  return client.rpc("provider.remove_account", { provider, account });
}

export interface ReprobeResult {
  report: {
    entries: Array<{ provider: string; display: string; status: string; detail: string }>;
    data_dir: string;
    config_dir: string;
  };
  outcomes: string[];
}

export function providerReprobe(): Promise<ReprobeResult> {
  return client.rpc("provider.reprobe");
}

export interface DispatchRecord {
  role: string;
  provider: string;
  model: string;
  degraded_from?: string | null;
  at: number;
}

export interface MrmStats {
  describe: string;
  history: DispatchRecord[];
}

export function mrmStats(): Promise<MrmStats> {
  return client.rpc("mrm.stats");
}

export interface TestDispatchResult {
  role: string;
  provider: string;
  model: string;
  account?: string | null;
  degraded_from?: string | null;
  answer: string;
}

export function testDispatch(role: string): Promise<TestDispatchResult> {
  return client.rpc("agent.test_dispatch", { role });
}
