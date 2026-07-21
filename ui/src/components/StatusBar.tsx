import { createSignal, Show, onCleanup, onMount } from "solid-js";
import { GitBranch, Target, ListTodo } from "lucide-solid";
import { statusline, type StatuslineReport } from "../lib/chat";
import { activeSessionId } from "../lib/state";

/** 底部状态栏：固定段 + config 开关，3s 轮询 + 事件驱动。 */
export default function StatusBar() {
  const [report, setReport] = createSignal<StatuslineReport | null>(null);
  let timer: ReturnType<typeof setInterval> | undefined;

  const reload = async () => {
    const r = await statusline(activeSessionId()).catch(() => null);
    if (r) setReport(r);
  };

  onMount(async () => {
    await reload();
    timer = setInterval(() => void reload(), 3000);
  });
  onCleanup(() => timer && clearInterval(timer));

  const has = (item: string) => report()?.items.includes(item) ?? false;
  const shortWorkdir = () => {
    const w = report()?.workdir ?? "";
    const home = "/Users/";
    const idx = w.indexOf(home);
    return idx === 0 ? `~${w.slice(home.length).split("/").slice(1).join("/")}` : w;
  };

  return (
    <div class="h-7 shrink-0 flex items-center gap-3 px-3 border-t border-[var(--border)] bg-[var(--bg-raised)] text-xs text-[var(--text-dim)] select-none">
      <Show when={has("workdir")}>
        <span class="truncate max-w-60" title={report()?.workdir}>
          {shortWorkdir()}
        </span>
      </Show>
      <Show when={has("git") && report()?.git_branch}>
        <span class="flex items-center gap-1">
          <GitBranch size={11} />
          {report()?.git_branch}
        </span>
      </Show>
      <Show when={has("goal") && report()?.goal}>
        <span class="flex items-center gap-1 text-[var(--accent-hover)]" title={report()?.goal?.id}>
          <Target size={11} />
          {report()?.goal?.status}
        </span>
      </Show>
      <Show when={has("tasks") && (report()?.tasks_running ?? 0) > 0}>
        <span class="flex items-center gap-1">
          <ListTodo size={11} />
          {report()?.tasks_running} 运行中
        </span>
      </Show>
      <span class="ml-auto flex items-center gap-3 tabular-nums">
        <Show when={has("tokens")}>
          <span title="本会话 tokens（input/output）">
            {report()?.tokens.input}/{report()?.tokens.output}
          </span>
        </Show>
        <Show when={has("model")}>
          <span class="text-[var(--text-faint)]">{report()?.model}</span>
        </Show>
      </span>
    </div>
  );
}
