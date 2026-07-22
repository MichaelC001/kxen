// 订阅状态台（多账号）：默认账号官方导入 + 命名账号手动添加；逐账号实测与修复指引。
import { createSignal, For, onMount, Show } from "solid-js";
import { Plus, RefreshCw, Trash2, Wrench } from "lucide-solid";
import { configGet } from "../../lib/chat";
import {
  importAccount,
  providerAccounts,
  providerReprobe,
  providerVerify,
  removeAccount,
  type AccountInfo,
  type VerifyOutcome,
} from "../../lib/provider";

interface Row extends AccountInfo {
  verify?: VerifyOutcome;
  verifying: boolean;
  usedBy: string[];
}

const PROVIDER_LABELS: Record<string, string> = {
  anthropic: "Claude Pro/Max",
  openai: "ChatGPT Plus/Pro",
  xai: "SuperGrok",
  "kimi-for-coding": "Kimi Code",
};

const GUIDES: Record<string, string[]> = {
  anthropic: [
    "1. 终端运行 `claude` 重新登录（订阅自动刷新到 Keychain）",
    "2. kxen 弹 keychain 读取请求时选「始终允许」",
    "3. 点「重新导入」",
  ],
  openai: ["1. 终端运行 `codex login` 重新登录", "2. 点「重新导入」"],
  xai: ["1. 终端运行 `grok` 触发登录刷新", "2. 点「重新导入」"],
  "kimi-for-coding": ["1. 终端运行 `kimi` 触发凭证刷新", "2. 点「重新导入」"],
};

export default function ProvidersSection() {
  const [rows, setRows] = createSignal<Row[]>([]);
  const [reprobing, setReprobing] = createSignal(false);
  const [note, setNote] = createSignal("");
  const [adding, setAdding] = createSignal(false);
  const [newProvider, setNewProvider] = createSignal("anthropic");
  const [newName, setNewName] = createSignal("");
  const [newToken, setNewToken] = createSignal("");
  const [guideFor, setGuideFor] = createSignal("");

  const load = async () => {
    const [accounts, cfg] = await Promise.all([
      providerAccounts().catch(() => []),
      configGet().catch(() => null),
    ]);
    const usedBy = new Map<string, string[]>();
    for (const [role, b] of Object.entries(cfg?.roles ?? {})) {
      const key = b.account ? `${b.provider}:${b.account}` : b.provider;
      usedBy.set(key, [...(usedBy.get(key) ?? []), role]);
    }
    setRows(accounts.map((a) => ({ ...a, verifying: false, usedBy: usedBy.get(a.id) ?? [] })));
  };

  const verifyOne = async (row: Row) => {
    setRows((prev) => prev.map((r) => (r.id === row.id ? { ...r, verifying: true } : r)));
    const account = row.account === "default" ? undefined : row.account;
    const v = await providerVerify(row.provider, account).catch((e) => ({
      ok: false,
      latency_ms: 0,
      detail: String(e),
    }));
    setRows((prev) =>
      prev.map((r) => (r.id === row.id ? { ...r, verifying: false, verify: v } : r)),
    );
  };

  const verifyAll = () => rows().forEach((r) => void verifyOne(r));

  onMount(async () => {
    await load();
    verifyAll();
  });

  const reprobe = async () => {
    setReprobing(true);
    try {
      const r = await providerReprobe();
      setNote(`已重新导入（${r.outcomes.join("，")}）`);
      await load();
      verifyAll();
    } finally {
      setReprobing(false);
      setTimeout(() => setNote(""), 4000);
    }
  };

  const addAccount = async () => {
    const name = newName().trim();
    const raw = newToken().trim();
    if (!name || !raw) return;
    // 支持整段 OAuth JSON 或裸 access token
    let access = raw;
    let refresh = "";
    let expires = 0;
    if (raw.startsWith("{")) {
      try {
        const j = JSON.parse(raw) as {
          access_token?: string;
          refresh_token?: string;
          expires_at?: number;
        };
        access = j.access_token ?? raw;
        refresh = j.refresh_token ?? "";
        expires = j.expires_at ?? 0;
      } catch {
        /* 按裸 token 处理 */
      }
    }
    await importAccount(newProvider(), name, access, refresh, expires);
    setNewName("");
    setNewToken("");
    setAdding(false);
    await load();
    setNote(`账号 ${newProvider()}:${name} 已添加`);
    setTimeout(() => setNote(""), 3000);
  };

  const remove = async (row: Row) => {
    await removeAccount(row.provider, row.account);
    await load();
    setNote(`已删除 ${row.id}`);
    setTimeout(() => setNote(""), 3000);
  };

  const badge = (r: Row) => {
    if (r.verifying) return { text: "实测中…", cls: "text-[var(--text-faint)]" };
    if (r.verify) {
      if (r.verify.ok)
        return {
          text: `实测正常 ${(r.verify.latency_ms / 1000).toFixed(1)}s`,
          cls: "text-[var(--ok)]",
        };
      return { text: "实测失败", cls: "text-[var(--err)]" };
    }
    if (r.expired) return { text: "已过期", cls: "text-[var(--err)]" };
    return { text: "凭证在位（未实测）", cls: "text-[var(--warn)]" };
  };

  return (
    <>
      <div class="flex items-center justify-between">
        <div class="text-xs text-[var(--text-faint)]">
          订阅实况（多账号 quota 池化；默认账号官方导入，命名账号手动添加）
        </div>
        <div class="flex items-center gap-1.5">
          <button
            class="pressable flex items-center gap-1 px-2 py-1 rounded text-xs text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60"
            onClick={() => setAdding(!adding())}
          >
            <Plus size={12} />
            添加账号
          </button>
          <button
            class="pressable flex items-center gap-1 px-2 py-1 rounded text-xs text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60"
            disabled={reprobing()}
            onClick={() => void reprobe()}
          >
            <RefreshCw size={12} class={reprobing() ? "animate-spin" : ""} />
            重新导入
          </button>
        </div>
      </div>
      <Show when={note()}>
        <div class="text-xs text-[var(--ok)]">{note()}</div>
      </Show>

      <Show when={adding()}>
        <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-3 space-y-2">
          <div class="flex gap-2">
            <select
              class="bg-transparent border border-[var(--border)] rounded px-1.5 py-1 text-xs text-[var(--text-dim)]"
              value={newProvider()}
              onChange={(e) => setNewProvider(e.currentTarget.value)}
            >
              {Object.entries(PROVIDER_LABELS).map(([id, label]) => (
                <option value={id}>{label}</option>
              ))}
            </select>
            <input
              class="flex-1 bg-transparent border border-[var(--border)] rounded px-2 py-1 text-xs"
              placeholder="账号名（如 work / personal）"
              value={newName()}
              onInput={(e) => setNewName(e.currentTarget.value)}
            />
          </div>
          <textarea
            class="w-full bg-transparent border border-[var(--border)] rounded px-2 py-1 text-xs font-mono h-14"
            placeholder="OAuth JSON（access_token/refresh_token/expires_at）或裸 access token"
            value={newToken()}
            onInput={(e) => setNewToken(e.currentTarget.value)}
          />
          <button
            class="pressable px-3 py-1 rounded-md text-xs border border-[var(--border)] disabled:opacity-40"
            disabled={!newName().trim() || !newToken().trim()}
            onClick={() => void addAccount()}
          >
            保存账号
          </button>
        </div>
      </Show>

      <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] divide-y divide-[var(--border)]">
        <For each={rows()}>
          {(r) => {
            const b = () => badge(r);
            return (
              <div class="px-4 py-3">
                <div class="flex items-center justify-between">
                  <div>
                    <div class="text-sm font-medium">
                      {PROVIDER_LABELS[r.provider] ?? r.provider}
                      <Show when={r.account !== "default"}>
                        <span class="text-[var(--text-faint)]"> · {r.account}</span>
                      </Show>
                    </div>
                    <div class="text-xs text-[var(--text-faint)]">
                      {r.id}
                      <Show when={r.usedBy.length > 0}> · 被 {r.usedBy.join("/")} 使用</Show>
                    </div>
                  </div>
                  <div class="flex items-center gap-2">
                    <div class={`text-sm font-medium ${b().cls}`}>{b().text}</div>
                    <button
                      class="pressable px-2 py-1 rounded text-2xs border border-[var(--border)]"
                      onClick={() => void verifyOne(r)}
                    >
                      实测
                    </button>
                    <Show when={r.account === "default"}>
                      <button
                        class="pressable px-2 py-1 rounded text-2xs border border-[var(--border)]"
                        onClick={() => setGuideFor(guideFor() === r.provider ? "" : r.provider)}
                      >
                        <Wrench size={11} />
                      </button>
                    </Show>
                    <Show when={r.account !== "default"}>
                      <button
                        class="pressable px-1.5 py-1 rounded text-[var(--text-faint)] hover:text-[var(--err)]"
                        onClick={() => void remove(r)}
                      >
                        <Trash2 size={12} />
                      </button>
                    </Show>
                  </div>
                </div>
                <Show when={r.verify && !r.verify.ok}>
                  <div class="mt-1.5 text-xs text-[var(--err)] break-all">{r.verify?.detail}</div>
                </Show>
                <Show when={guideFor() === r.provider && r.account === "default"}>
                  <div class="mt-2 rounded border border-[var(--border)] bg-[var(--bg-overlay)]/50 px-3 py-2 text-xs space-y-1">
                    <For each={GUIDES[r.provider] ?? []}>{(g) => <div>{g}</div>}</For>
                  </div>
                </Show>
              </div>
            );
          }}
        </For>
      </div>
    </>
  );
}
