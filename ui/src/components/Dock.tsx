import { createSignal, For, Show, onCleanup, onMount } from "solid-js";
import {
  diffFile,
  diffStatus,
  goalFocus,
  goalTransit,
  onTopic,
  taskKill,
  taskList,
  type DiffStatusEntry,
  type GoalInfo,
  type TaskInfo,
} from "../lib/chat";
import Markdown from "./Markdown";

const GOAL_STATUS: Record<string, { text: string; cls: string }> = {
  draft: { text: "草稿", cls: "text-[var(--text-dim)]" },
  queued: { text: "排队", cls: "text-[var(--text-dim)]" },
  active: { text: "进行中", cls: "text-[var(--accent-hover)]" },
  paused: { text: "已暂停", cls: "text-[var(--warn)]" },
  blocked: { text: "阻塞", cls: "text-[var(--err)]" },
  budgetlimited: { text: "预算耗尽", cls: "text-[var(--err)]" },
  complete: { text: "已完成", cls: "text-[var(--ok)]" },
  canceled: { text: "已取消", cls: "text-[var(--text-faint)]" },
};

const TASK_STATUS: Record<string, string> = {
  running: "text-[var(--ok)]",
  exited: "text-[var(--text-dim)]",
  killed: "text-[var(--warn)]",
  failed: "text-[var(--err)]",
};

function Section(props: { title: string; children: unknown }) {
  return (
    <div class="border-b border-[var(--border)] px-3 py-3">
      <div class="text-[10px] uppercase tracking-wider text-[var(--text-faint)] mb-2">
        {props.title}
      </div>
      {props.children}
    </div>
  );
}

/** 右 dock：会话上下文（目标 / 改动 / 后台任务）。 */
export default function Dock() {
  const [goal, setGoal] = createSignal<GoalInfo | null>(null);
  const [changes, setChanges] = createSignal<DiffStatusEntry[]>([]);
  const [tasks, setTasks] = createSignal<TaskInfo[]>([]);
  const [openDiff, setOpenDiff] = createSignal<{ path: string; text: string } | null>(null);
  let unlisten: (() => void) | undefined;
  let timer: ReturnType<typeof setInterval> | undefined;

  const reloadGoal = async () => setGoal(await goalFocus());
  const reloadDiff = async () => setChanges(await diffStatus().catch(() => []));
  const reloadTasks = async () => setTasks(await taskList());

  onMount(async () => {
    await Promise.all([reloadGoal(), reloadDiff(), reloadTasks()]);
    unlisten = await onTopic(["goal.update", "task.update"], () => {
      void reloadGoal();
      void reloadTasks();
    });
    timer = setInterval(() => {
      void reloadDiff();
      void reloadTasks();
    }, 3000);
  });
  onCleanup(() => {
    unlisten?.();
    if (timer) clearInterval(timer);
  });

  const act = async (action: "activate" | "pause" | "resume" | "cancel") => {
    const g = goal();
    if (!g) return;
    await goalTransit(g.id, action);
    await reloadGoal();
  };

  const toggleDiff = async (path: string) => {
    if (openDiff()?.path === path) {
      setOpenDiff(null);
      return;
    }
    const text = await diffFile(path).catch(() => "");
    setOpenDiff({ path, text });
  };

  const badge = () => GOAL_STATUS[goal()?.status ?? ""] ?? { text: "", cls: "" };

  return (
    <aside class="w-64 shrink-0 border-l border-[var(--border)] bg-[var(--bg-raised)] overflow-y-auto">
      <Section title="目标">
        <Show
          when={goal()}
          fallback={
            <div class="text-xs text-[var(--text-faint)]">
              无焦点 goal。会话里说 write-goal 创建。
            </div>
          }
        >
          {(g) => (
            <div class="space-y-1.5">
              <div class="flex items-center gap-1.5">
                <span class={`text-xs font-medium ${badge().cls}`}>{badge().text}</span>
                <span class="text-[10px] text-[var(--text-faint)]">
                  turns {g().turns_used}
                  {g().budget.turns ? `/${g().budget.turns}` : ""}
                </span>
              </div>
              <div class="text-xs leading-snug">{g().objective}</div>
              <div class="text-[10px] text-[var(--text-dim)]">判据：{g().completion_criteria}</div>
              <Show when={g().block_reason}>
                <div class="text-[10px] text-[var(--err)]">阻塞：{g().block_reason}</div>
              </Show>
              <div class="flex gap-1.5 pt-0.5">
                <Show when={g().status === "active"}>
                  <button
                    class="pressable px-2 py-0.5 rounded text-[10px] border border-[var(--border)] text-[var(--warn)]"
                    onClick={() => void act("pause")}
                  >
                    暂停
                  </button>
                </Show>
                <Show when={["paused", "blocked", "budgetlimited"].includes(g().status)}>
                  <button
                    class="pressable px-2 py-0.5 rounded text-[10px] bg-[var(--accent)] text-white"
                    onClick={() => void act("resume")}
                  >
                    恢复
                  </button>
                </Show>
                <Show when={["draft", "queued"].includes(g().status)}>
                  <button
                    class="pressable px-2 py-0.5 rounded text-[10px] bg-[var(--accent)] text-white"
                    onClick={() => void act("activate")}
                  >
                    激活
                  </button>
                </Show>
                <button
                  class="pressable px-2 py-0.5 rounded text-[10px] border border-[var(--border)] text-[var(--err)]"
                  onClick={() => void act("cancel")}
                >
                  取消
                </button>
              </div>
            </div>
          )}
        </Show>
      </Section>

      <Section title="改动">
        <Show
          when={changes().length > 0}
          fallback={<div class="text-xs text-[var(--text-faint)]">workdir 无未提交改动</div>}
        >
          <div class="space-y-0.5">
            <For each={changes()}>
              {(c) => (
                <div>
                  <button
                    class="w-full flex items-center gap-1.5 px-1 py-0.5 rounded text-xs text-left hover:bg-[var(--bg-overlay)]/60"
                    onClick={() => void toggleDiff(c.path)}
                  >
                    <span
                      class="font-mono text-[10px] w-6 shrink-0"
                      classList={{
                        "text-[var(--ok)]": c.status === "??" || c.status === "A",
                        "text-[var(--warn)]": c.status === "M",
                        "text-[var(--err)]": c.status === "D",
                      }}
                    >
                      {c.status}
                    </span>
                    <span class="truncate font-mono text-[var(--text-dim)]">{c.path}</span>
                  </button>
                  <Show when={openDiff()?.path === c.path}>
                    <div class="mt-1 mb-2 text-[10px] max-h-72 overflow-auto rounded border border-[var(--border)]">
                      <Markdown text={"```diff\n" + (openDiff()?.text ?? "") + "\n```"} />
                    </div>
                  </Show>
                </div>
              )}
            </For>
          </div>
        </Show>
      </Section>

      <Section title="后台任务">
        <Show
          when={tasks().length > 0}
          fallback={<div class="text-xs text-[var(--text-faint)]">无后台任务</div>}
        >
          <div class="space-y-1.5">
            <For each={tasks()}>
              {(t) => (
                <div class="text-xs space-y-0.5">
                  <div class="flex items-center gap-1.5">
                    <span class={`text-[10px] font-medium ${TASK_STATUS[t.status] ?? ""}`}>
                      {t.status}
                    </span>
                    <Show when={t.port}>
                      <a
                        class="text-[10px] text-[var(--accent-hover)]"
                        href={`http://localhost:${t.port}`}
                        target="_blank"
                        rel="noreferrer"
                      >
                        :{t.port}
                      </a>
                    </Show>
                    <Show when={t.status === "running"}>
                      <button
                        class="pressable ml-auto px-1.5 py-0 rounded text-[10px] border border-[var(--border)] text-[var(--err)]"
                        onClick={() => void taskKill(t.id).then(reloadTasks)}
                      >
                        终止
                      </button>
                    </Show>
                  </div>
                  <div
                    class="font-mono text-[10px] text-[var(--text-dim)] truncate"
                    title={t.command}
                  >
                    {t.command}
                  </div>
                </div>
              )}
            </For>
          </div>
        </Show>
      </Section>
    </aside>
  );
}
