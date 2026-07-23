// 订阅状态台（多账号）：默认账号官方导入 + 命名账号手动添加；逐账号实测与修复指引。
import { createSignal, For, onMount, Show } from "solid-js";
import { Plus, RefreshCw, Trash2, Wrench } from "lucide-solid";
import { configGet } from "../../lib/chat";
import {
  providerAccounts,
  providerModels,
  providerReprobe,
  providerVerify,
  removeAccount,
  type AccountInfo,
  type VerifyOutcome,
} from "../../lib/provider";
import AddAccountPanel from "./AddAccountPanel";

interface Row extends AccountInfo {
  verify?: VerifyOutcome;
  verifying: boolean;
  usedBy: string[];
  modelCount?: number;
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

  /** 手动拉取模型清单（端点 /models），条数就地显示。 */
  const fetchModels = async (row: Row) => {
    const account = row.account === "default" ? undefined : row.account;
    const r = await providerModels(row.provider, account).catch(() => null);
    setRows((prev) =>
      prev.map((x) =>
        x.id === row.id ? { ...x, modelCount: r && r.models.length > 0 ? r.models.length : 0 } : x,
      ),
    );
  };

  const verifyAll = () => rows().forEach((r) => void verifyOne(r));

  // 打开页面零自动请求（同类型产品共识：探测只在首次导入 + 用户主动点）
  onMount(() => void load());

  const reprobe = async () => {
    setReprobing(true);
    try {
      const r = await providerReprobe();
      setNote(`已重新导入（${r.outcomes.join("，")}）`);
      await load();
      verifyAll(); // 重新导入 = 用户主动动作，导入后逐个验证一次
    } finally {
      setReprobing(false);
      setTimeout(() => setNote(""), 4000);
    }
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
        <AddAccountPanel
          onDone={(msg) => {
            setAdding(false);
            setNote(msg);
            setTimeout(() => setNote(""), 3000);
            void load();
          }}
        />
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
                    <button
                      class="pressable px-2 py-1 rounded text-2xs border border-[var(--border)]"
                      title="从端点拉取模型清单"
                      onClick={() => void fetchModels(r)}
                    >
                      拉模型
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
                <Show when={r.modelCount !== undefined}>
                  <div class="mt-1 text-2xs text-[var(--text-faint)]">
                    端点模型：{r.modelCount} 个（已并入 composer 模型选择器）
                  </div>
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
