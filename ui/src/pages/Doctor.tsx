import { createSignal, For, Show } from 'solid-js';
import { doctor, type DoctorReport } from '../lib/tauri';

const STATUS_STYLE: Record<string, string> = {
  imported: 'text-emerald-400',
  ok: 'text-emerald-400',
  missing: 'text-amber-400',
  expired: 'text-rose-400',
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

  run();

  return (
    <div class="p-6 max-w-2xl">
      <div class="flex items-center justify-between mb-4">
        <h1 class="text-xl font-semibold">Doctor</h1>
        <button
          class="px-3 py-1 rounded bg-indigo-600 hover:bg-indigo-500 text-sm disabled:opacity-50"
          onClick={run}
          disabled={loading()}
        >
          {loading() ? '检查中…' : '重新检查'}
        </button>
      </div>

      <Show when={report()}>
        {(r) => (
          <div class="space-y-4">
            <div class="text-sm text-gray-400 space-y-1">
              <div>{r().bun_like_runtime}</div>
              <div>data: {r().data_dir}</div>
              <div>config: {r().config_dir}</div>
            </div>
            <div class="border border-gray-800 rounded-lg divide-y divide-gray-800">
              <For each={r().entries}>
                {(entry) => (
                  <div class="flex items-center justify-between px-4 py-3">
                    <div>
                      <div class="font-medium">{entry.display}</div>
                      <div class="text-xs text-gray-500">{entry.provider}</div>
                    </div>
                    <div class="text-right">
                      <div class={`text-sm font-medium ${STATUS_STYLE[entry.status] ?? ''}`}>
                        {entry.status}
                      </div>
                      <div class="text-xs text-gray-500">{entry.detail}</div>
                    </div>
                  </div>
                )}
              </For>
            </div>
          </div>
        )}
      </Show>
    </div>
  );
}
