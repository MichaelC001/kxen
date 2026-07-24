// 定时任务区块：cron 表达式 / 目标会话 / 下次触发 / 最近执行状态，暂停/恢复/删除。
import { createSignal, For, onMount, Show } from "solid-js";
import { Pause, Play, Trash2 } from "lucide-solid";
import { relTime } from "../../lib/time";
import {
  scheduleList,
  scheduleRemove,
  scheduleSetEnabled,
  type ScheduleJob,
} from "../../lib/schedule";

export default function ScheduleSection() {
  const [jobs, setJobs] = createSignal<ScheduleJob[]>([]);
  const reload = async () => setJobs(await scheduleList().catch(() => []));
  onMount(() => void reload());

  const toggle = async (job: ScheduleJob) => {
    await scheduleSetEnabled(job.id, !job.enabled).catch(() => {});
    void reload();
  };
  const remove = async (job: ScheduleJob) => {
    await scheduleRemove(job.id).catch(() => {});
    void reload();
  };

  const fmtFire = (ms: number) => {
    const d = new Date(ms);
    const hm = `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
    return `${d.getMonth() + 1}/${d.getDate()} ${hm}`;
  };

  return (
    <div class="rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] divide-y divide-[var(--border)]">
      <Show
        when={jobs().length > 0}
        fallback={
          <div class="px-4 py-3 text-xs text-[var(--text-faint)]">
            暂无定时任务（由 agent 的 schedule 工具创建）
          </div>
        }
      >
        <For each={jobs()}>
          {(job) => (
            <div class="px-4 py-3">
              <div class="flex items-center gap-2">
                <span class="font-mono text-xs text-[var(--text)]">{job.cron}</span>
                <Show when={job.once}>
                  <span class="text-2xs text-[var(--text-faint)]">一次性</span>
                </Show>
                <Show when={!job.enabled}>
                  <span class="text-2xs text-[var(--text-faint)]">已暂停</span>
                </Show>
                <span class="ml-auto text-2xs text-[var(--text-faint)]">
                  下次 {fmtFire(job.next_fire)}
                </span>
                <button
                  class="pressable px-2 py-0.5 rounded border border-[var(--border)] text-xs text-[var(--text)] flex items-center gap-1"
                  onClick={() => void toggle(job)}
                >
                  {job.enabled ? <Pause size={10} /> : <Play size={10} />}
                  {job.enabled ? "暂停" : "恢复"}
                </button>
                <button
                  class="pressable px-2 py-0.5 rounded border border-[var(--border)] text-xs text-[var(--text)] flex items-center gap-1"
                  onClick={() => void remove(job)}
                >
                  <Trash2 size={10} />
                  删除
                </button>
              </div>
              <div class="mt-1 text-xs text-[var(--text-dim)] truncate">{job.prompt}</div>
              <div class="mt-1 flex items-center gap-3 text-2xs text-[var(--text-faint)]">
                <span class="truncate">会话 {job.session_id}</span>
                <Show when={job.history[0]} fallback={<span>尚未执行</span>}>
                  {(rec) => (
                    <span style={{ color: rec().ok ? "var(--ok)" : "var(--err, #e5534b)" }}>
                      最近执行 {relTime(rec().at)}
                      {rec().ok ? " 成功" : ` 失败：${rec().error ?? ""}`}
                    </span>
                  )}
                </Show>
              </div>
            </div>
          )}
        </For>
      </Show>
    </div>
  );
}
