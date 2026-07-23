// 用量与统计：usage.overview 真实数据（tokens 汇总 + 模型派发分布）。
import { createSignal, For, onMount, Show } from "solid-js";
import { client } from "../../lib/client";

interface Overview {
  total_input: number;
  total_output: number;
  sessions: number;
  dispatches: number;
  by_model: Record<string, number>;
}

export default function UsageSection() {
  const [data, setData] = createSignal<Overview | null>(null);
  onMount(async () => {
    setData(await client.rpc<Overview>("usage.overview").catch(() => null));
  });

  const models = () => Object.entries(data()?.by_model ?? {}).sort((a, b) => b[1] - a[1]);
  const fmt = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n));

  return (
    <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] divide-y divide-[var(--border)]">
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
    </div>
  );
}
