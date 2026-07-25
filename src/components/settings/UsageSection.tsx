// 用量与统计：usage.overview 真实数据（tokens 汇总 + 模型派发分布）。
// RPC 失败与真零严格区分：失败显错误态，不把加载失败渲成全零。
import { createSignal, For, onMount, Show } from "solid-js";
import { client } from "../../lib/client";
import { formatError } from "../../lib/error-text";

interface Overview {
  total_input: number;
  total_output: number;
  sessions: number;
  dispatches: number;
  by_model: Record<string, number>;
}

export default function UsageSection() {
  const [data, setData] = createSignal<Overview | null>(null);
  const [loadErr, setLoadErr] = createSignal("");

  const load = async () => {
    const r = await client.rpc<Overview>("usage.overview").catch((e: unknown) => {
      setLoadErr(formatError(e instanceof Error ? e.message : String(e)));
      return null;
    });
    if (r) {
      setData(r);
      setLoadErr("");
    }
  };
  onMount(() => void load());

  const models = () => Object.entries(data()?.by_model ?? {}).sort((a, b) => b[1] - a[1]);
  const fmt = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n));

  return (
    <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] divide-y divide-[var(--border)]">
      <Show when={loadErr()}>
        <div class="px-4 py-3 text-xs flex items-center gap-3">
          <span class="text-[var(--err)]">加载用量统计失败：{loadErr()}</span>
          <button
            class="pressable px-2 py-0.5 rounded border border-[var(--border)] text-[var(--text-dim)]"
            onClick={() => void load()}
          >
            重试
          </button>
        </div>
      </Show>
      <Show when={!data() && !loadErr()}>
        <div class="px-4 py-3 text-xs text-[var(--text-faint)]">加载中…</div>
      </Show>
      <Show when={data()}>
        <div class="grid grid-cols-4 gap-2 px-4 py-3">
          <div>
            <div class="text-2xs text-[var(--text-faint)]">输入 tokens</div>
            <div class="text-sm tabular-nums">{fmt(data()?.total_input ?? 0)}</div>
          </div>
          <div>
            <div class="text-2xs text-[var(--text-faint)]">输出 tokens</div>
            <div class="text-sm tabular-nums">{fmt(data()?.total_output ?? 0)}</div>
          </div>
          <div>
            <div class="text-2xs text-[var(--text-faint)]">会话</div>
            <div class="text-sm tabular-nums">{data()?.sessions ?? 0}</div>
          </div>
          <div>
            <div class="text-2xs text-[var(--text-faint)]">派发次数</div>
            <div class="text-sm tabular-nums">{data()?.dispatches ?? 0}</div>
          </div>
        </div>
        <div class="px-4 py-3">
          <div class="text-2xs text-[var(--text-faint)] mb-2">按模型的派发分布</div>
          <Show
            when={models().length > 0}
            fallback={<div class="text-xs text-[var(--text-faint)]">暂无派发记录</div>}
          >
            <div class="space-y-1">
              <For each={models()}>
                {([name, count]) => (
                  <div class="flex items-center gap-2 text-xs">
                    <span class="flex-1 truncate font-mono text-[var(--text-dim)]">{name}</span>
                    <span class="tabular-nums text-[var(--text-faint)]">{count} 次</span>
                  </div>
                )}
              </For>
            </div>
          </Show>
          <div class="text-2xs text-[var(--text-faint)] mt-3">
            每条 assistant 消息尾部有 TTFT / 耗时 / tok/s；tokens 统计自本次启动起累计。
          </div>
        </div>
      </Show>
    </div>
  );
}
