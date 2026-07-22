// 订阅状态台：文件级状态 + 实况实测（live call）+ 修复指引 + 重新导入。文件新鲜 ≠ token 有效。
import { createSignal, For, onMount, Show } from "solid-js";
import { CheckCircle2, RefreshCw, Wrench, XCircle } from "lucide-solid";
import { configGet, doctor } from "../../lib/chat";
import { providerReprobe, providerVerify, type VerifyOutcome } from "../../lib/provider";

interface Row {
  provider: string;
  display: string;
  fileStatus: string;
  fileDetail: string;
  usedBy: string[];
  verify?: VerifyOutcome;
  verifying: boolean;
  showGuide: boolean;
}

const GUIDES: Record<string, string[]> = {
  anthropic: [
    "1. 终端运行 `claude` 重新登录（订阅自动刷新到 Keychain）",
    "2. kxen 弹 keychain 读取请求时选「始终允许」",
    "3. 回到本页点「重新导入」",
  ],
  openai: ["1. 终端运行 `codex login` 重新登录", "2. 回到本页点「重新导入」"],
  xai: [
    "1. 终端运行 `grok` 触发登录刷新（~/.grok/auth.json 自动轮换）",
    "2. 回到本页点「重新导入」",
  ],
  "kimi-for-coding": ["1. 终端运行 `kimi` 触发凭证刷新", "2. 回到本页点「重新导入」"],
};

export default function ProvidersSection() {
  const [rows, setRows] = createSignal<Row[]>([]);
  const [reprobing, setReprobing] = createSignal(false);
  const [note, setNote] = createSignal("");

  const load = async () => {
    const [rep, cfg] = await Promise.all([
      doctor().catch(() => null),
      configGet().catch(() => null),
    ]);
    const usedBy = new Map<string, string[]>();
    for (const [role, b] of Object.entries(cfg?.roles ?? {})) {
      usedBy.set(b.provider, [...(usedBy.get(b.provider) ?? []), role]);
    }
    setRows(
      (rep?.entries ?? []).map((e) => ({
        provider: e.provider,
        display: e.display,
        fileStatus: e.status,
        fileDetail: e.detail,
        usedBy: usedBy.get(e.provider) ?? [],
        verifying: false,
        showGuide: false,
      })),
    );
  };

  const verifyAll = async (list: Row[]) => {
    for (const row of list) {
      setRows((prev) =>
        prev.map((r) => (r.provider === row.provider ? { ...r, verifying: true } : r)),
      );
      const v = await providerVerify(row.provider).catch((e) => ({
        ok: false,
        latency_ms: 0,
        detail: String(e),
      }));
      setRows((prev) =>
        prev.map((r) => (r.provider === row.provider ? { ...r, verifying: false, verify: v } : r)),
      );
    }
  };

  onMount(async () => {
    await load();
    void verifyAll(rows());
  });

  const reprobe = async () => {
    setReprobing(true);
    try {
      const r = await providerReprobe();
      setNote(`已重新导入（${r.outcomes.join("，")}）`);
      await load();
      void verifyAll(rows());
    } finally {
      setReprobing(false);
      setTimeout(() => setNote(""), 4000);
    }
  };

  const badge = (r: Row) => {
    if (r.verifying) return { text: "实测中…", cls: "text-[var(--text-faint)]" };
    if (r.verify) {
      if (r.verify.ok)
        return {
          text: `实测正常 ${(r.verify.latency_ms / 1000).toFixed(1)}s`,
          cls: "text-[var(--ok)]",
        };
      if (r.fileStatus === "ok") return { text: "文件有效但实测失败", cls: "text-[var(--err)]" };
      return { text: "实测失败", cls: "text-[var(--err)]" };
    }
    if (r.fileStatus === "ok") return { text: "凭证在位（未实测）", cls: "text-[var(--warn)]" };
    return { text: r.fileStatus === "expired" ? "已过期" : "缺失", cls: "text-[var(--err)]" };
  };

  return (
    <>
      <div class="flex items-center justify-between">
        <div class="text-xs text-[var(--text-faint)]">
          订阅实况（启动自动实测一次；文件状态与真实调用分列）
        </div>
        <button
          class="pressable flex items-center gap-1 px-2 py-1 rounded text-xs text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60"
          disabled={reprobing()}
          onClick={() => void reprobe()}
        >
          <RefreshCw size={12} class={reprobing() ? "animate-spin" : ""} />
          重新导入
        </button>
      </div>
      <Show when={note()}>
        <div class="text-xs text-[var(--ok)]">{note()}</div>
      </Show>
      <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] divide-y divide-[var(--border)]">
        <For each={rows()}>
          {(r) => {
            const b = () => badge(r);
            return (
              <div class="px-4 py-3">
                <div class="flex items-center justify-between">
                  <div>
                    <div class="text-sm font-medium">{r.display}</div>
                    <div class="text-xs text-[var(--text-faint)]">
                      {r.provider}
                      <Show when={r.usedBy.length > 0}> · 被 {r.usedBy.join("/")} 使用</Show>
                    </div>
                  </div>
                  <div class="flex items-center gap-2">
                    <div class={`text-sm font-medium ${b().cls} flex items-center gap-1`}>
                      <Show
                        when={r.verify?.ok}
                        fallback={r.verify && !r.verify.ok && <XCircle size={13} />}
                      >
                        <CheckCircle2 size={13} />
                      </Show>
                      {b().text}
                    </div>
                    <button
                      class="pressable px-2 py-1 rounded text-2xs border border-[var(--border)]"
                      onClick={() => void verifyAll([r])}
                    >
                      实测
                    </button>
                    <button
                      class="pressable px-2 py-1 rounded text-2xs border border-[var(--border)]"
                      onClick={() =>
                        setRows((prev) =>
                          prev.map((x) =>
                            x.provider === r.provider ? { ...x, showGuide: !x.showGuide } : x,
                          ),
                        )
                      }
                    >
                      <Wrench size={11} />
                    </button>
                  </div>
                </div>
                <Show when={r.verify && !r.verify.ok}>
                  <div class="mt-1.5 text-xs text-[var(--err)] break-all">{r.verify?.detail}</div>
                </Show>
                <Show when={r.showGuide}>
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
