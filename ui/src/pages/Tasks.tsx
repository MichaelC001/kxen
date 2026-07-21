import { createSignal, For, Show, onMount, onCleanup } from "solid-js";
import { taskKill, taskList, type TaskInfo } from "../lib/chat";

const STATUS_LABEL: Record<string, { text: string; cls: string }> = {
  running: { text: "运行中", cls: "text-[var(--ok)]" },
  exited: { text: "已退出", cls: "text-[var(--text-dim)]" },
  killed: { text: "已终止", cls: "text-[var(--warn)]" },
  failed: { text: "失败", cls: "text-[var(--err)]" },
};

function formatUptime(ms: number): string {
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m${s % 60}s`;
  return `${Math.floor(m / 60)}h${m % 60}m`;
}

export default function Tasks() {
  const [tasks, setTasks] = createSignal<TaskInfo[]>([]);
  let timer: ReturnType<typeof setInterval> | undefined;

  const reload = async () => setTasks(await taskList());

  onMount(async () => {
    await reload();
    timer = setInterval(() => void reload(), 3000);
  });
  onCleanup(() => timer && clearInterval(timer));

  const kill = async (id: string) => {
    await taskKill(id);
    await reload();
  };

  return (
    <div class="h-full overflow-auto p-6">
      <div class="max-w-3xl mx-auto space-y-4">
        <div class="flex items-center justify-between">
          <h1 class="text-lg font-semibold">后台任务</h1>
          <span class="text-xs text-[var(--text-faint)]">dev server / 长命令，3s 自动刷新</span>
        </div>
        <Show when={tasks().length === 0}>
          <div class="text-sm text-[var(--text-faint)] text-center mt-16">
            没有后台任务。agent 启动的 dev server 和长命令会出现在这里。
          </div>
        </Show>
        <div class="space-y-2">
          <For each={tasks()}>
            {(t) => {
              const badge = () => STATUS_LABEL[t.status] ?? { text: t.status, cls: "" };
              return (
                <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-3 space-y-1.5">
                  <div class="flex items-center gap-2 text-xs">
                    <span class={`font-medium ${badge().cls}`}>{badge().text}</span>
                    <span class="font-mono text-[var(--text-faint)]">{t.id}</span>
                    <span class="text-[var(--text-faint)]">{formatUptime(t.uptime_ms)}</span>
                    <Show when={t.port}>
                      <a
                        class="text-[var(--accent-hover)]"
                        href={`http://localhost:${t.port}`}
                        target="_blank"
                        rel="noreferrer"
                      >
                        :{t.port}
                      </a>
                    </Show>
                    <Show when={t.status === "running"}>
                      <button
                        class="pressable ml-auto px-2 py-0.5 rounded text-[10px] border border-[var(--border)] text-[var(--err)]"
                        onClick={() => void kill(t.id)}
                      >
                        终止
                      </button>
                    </Show>
                  </div>
                  <div class="text-xs font-mono text-[var(--text-dim)] truncate" title={t.command}>
                    {t.command}
                  </div>
                  <Show when={t.tail}>
                    <pre class="text-[10px] text-[var(--text-faint)] whitespace-pre-wrap break-all max-h-24 overflow-auto border-t border-[var(--border)] pt-1">
                      {t.tail}
                    </pre>
                  </Show>
                </div>
              );
            }}
          </For>
        </div>
      </div>
    </div>
  );
}
