import { createSignal, For, Show, onMount } from "solid-js";
import { doctor, type DoctorReport } from "../lib/chat";

const STATUS_STYLE: Record<string, { text: string; cls: string }> = {
  imported: { text: "已导入", cls: "text-[var(--ok)]" },
  ok: { text: "正常", cls: "text-[var(--ok)]" },
  missing: { text: "缺失", cls: "text-[var(--warn)]" },
  expired: { text: "过期", cls: "text-[var(--err)]" },
};

export default function Doctor() {
  const [report, setReport] = createSignal<DoctorReport | null>(null);
  const [loading, setLoading] = createSignal(false);

  const run = async () => {
    setLoading(true);
    try {
      setReport(await doctor());
    } finally {
      setLoading(false);
    }
  };

  onMount(() => void run());

  return (
    <div class="h-full overflow-auto">
      <div class="h-8" data-tauri-drag-region />
      <div class="p-6 pt-2">
        <div class="max-w-2xl mx-auto space-y-4">
          <div class="flex items-center justify-between">
            <h1 class="text-lg font-semibold">环境检查</h1>
            <button
              class="pressable px-3 py-1 rounded-md bg-[var(--accent)] hover:bg-[var(--accent-hover)] text-sm text-white disabled:opacity-50"
              onClick={() => void run()}
              disabled={loading()}
            >
              {loading() ? "检查中…" : "重新检查"}
            </button>
          </div>

          <Show when={report()}>
            {(r) => (
              <div class="space-y-4">
                <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-3 text-xs text-[var(--text-dim)] space-y-1 font-mono">
                  <div>{r().bun_like_runtime}</div>
                  <div>data: {r().data_dir}</div>
                  <div>config: {r().config_dir}</div>
                </div>
                <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] divide-y divide-[var(--border)]">
                  <For each={r().entries}>
                    {(entry) => {
                      const badge = () =>
                        STATUS_STYLE[entry.status] ?? { text: entry.status, cls: "" };
                      return (
                        <div class="flex items-center justify-between px-4 py-3">
                          <div>
                            <div class="text-sm font-medium">{entry.display}</div>
                            <div class="text-xs text-[var(--text-faint)]">{entry.provider}</div>
                          </div>
                          <div class="text-right">
                            <div class={`text-sm font-medium ${badge().cls}`}>{badge().text}</div>
                            <div class="text-xs text-[var(--text-faint)]">{entry.detail}</div>
                          </div>
                        </div>
                      );
                    }}
                  </For>
                </div>
              </div>
            )}
          </Show>
        </div>
      </div>
    </div>
  );
}
