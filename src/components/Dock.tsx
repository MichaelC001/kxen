import { createSignal, For, Show, onCleanup, onMount } from "solid-js";
import {
  agentDiffFile,
  agentDiffStatus,
  goalFocus,
  goalList,
  goalTransit,
  onTopic,
  taskKill,
  taskList,
  type AgentDiffEntry,
  type GoalInfo,
  type TaskInfo,
} from "../lib/chat";
import { client } from "../lib/client";
import { createAction } from "../lib/async-guard";
import { activeSessionId } from "../lib/state";
import Markdown from "./Markdown";
import DockWorktree from "./DockWorktree";
import { FileDiff, SquareTerminal, Target } from "lucide-solid";

const GOAL_STATUS: Record<string, { text: string; cls: string }> = {
  draft: { text: "草稿", cls: "text-[var(--text-dim)]" },
  queued: { text: "排队", cls: "text-[var(--text-dim)]" },
  active: { text: "进行中", cls: "text-[var(--accent-hover)]" },
  paused: { text: "已暂停", cls: "text-[var(--warn)]" },
  blocked: { text: "阻塞", cls: "text-[var(--err)]" },
  budget_limited: { text: "预算耗尽", cls: "text-[var(--err)]" },
  complete: { text: "已完成", cls: "text-[var(--ok)]" },
  canceled: { text: "已取消", cls: "text-[var(--text-faint)]" },
};

const TASK_STATUS: Record<string, string> = {
  running: "text-[var(--ok)]",
  exited: "text-[var(--text-dim)]",
  killed: "text-[var(--warn)]",
  failed: "text-[var(--err)]",
};

/** 展开日志 tail 的任务 id（dock 单例，模块级信号即可）。 */
const [openTask, setOpenTask] = createSignal("");

function Section(props: {
  title: string;
  icon: (p: { size: number; class?: string }) => import("solid-js").JSX.Element;
  children: import("solid-js").JSX.Element;
}) {
  const Icon = props.icon;
  return (
    <div class="border-b border-[var(--border)] px-3 py-3">
      <div class="text-2xs uppercase tracking-wider text-[var(--text-faint)] mb-2 flex items-center gap-1.5">
        <Icon size={11} class="text-[var(--text-faint)]" />
        {props.title}
      </div>
      {props.children}
    </div>
  );
}

/** 右 dock：会话上下文（目标 / 改动 / 后台任务）。 */
function DockSections(props: {
  goal: GoalInfo | null;
  badge: () => { text: string; cls: string };
  act: (action: "activate" | "pause" | "resume" | "cancel") => void;
  acting: () => boolean;
  changes: AgentDiffEntry[];
  openDiff: { path: string; text: string } | null;
  toggleDiff: (path: string) => void;
  tasks: TaskInfo[];
  reloadTasks: () => void;
}) {
  const goal = () => props.goal;
  const badge = props.badge;
  const act = props.act;
  const acting = props.acting;
  const reloadTasks = props.reloadTasks;
  const changes = () => props.changes;
  const openDiff = () => props.openDiff;
  const toggleDiff = props.toggleDiff;
  const tasks = () => props.tasks;
  return (
    <>
      <Section title="目标" icon={Target}>
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
                <span class="text-2xs text-[var(--text-faint)]">
                  turns {g().turns_used}
                  {g().budget.turns ? `/${g().budget.turns}` : ""}
                </span>
              </div>
              <div class="text-xs leading-snug">{g().objective}</div>
              <div class="text-2xs text-[var(--text-dim)]">判据：{g().completion_criteria}</div>
              <Show when={g().block_reason}>
                <div class="text-2xs text-[var(--err)]">阻塞：{g().block_reason}</div>
              </Show>
              <Show when={g().verification_evidence}>
                <details class="text-2xs text-[var(--text-dim)]">
                  <summary class="cursor-pointer select-none">验证证据</summary>
                  <div class="mt-0.5 whitespace-pre-wrap break-words">
                    {g().verification_evidence}
                  </div>
                </details>
              </Show>
              <div class="flex gap-1.5 pt-0.5">
                <Show when={g().status === "active"}>
                  <button
                    class="pressable px-2 py-0.5 rounded text-2xs border border-[var(--border)] text-[var(--warn)] disabled:opacity-50"
                    disabled={acting()}
                    onClick={() => act("pause")}
                  >
                    暂停
                  </button>
                </Show>
                <Show when={["paused", "blocked", "budget_limited"].includes(g().status)}>
                  <button
                    class="pressable px-2 py-0.5 rounded text-2xs bg-[var(--accent)] text-white disabled:opacity-50"
                    disabled={acting()}
                    onClick={() => act("resume")}
                  >
                    恢复
                  </button>
                </Show>
                <Show when={["draft", "queued"].includes(g().status)}>
                  <button
                    class="pressable px-2 py-0.5 rounded text-2xs bg-[var(--accent)] text-white disabled:opacity-50"
                    disabled={acting()}
                    onClick={() => act("activate")}
                  >
                    激活
                  </button>
                </Show>
                <Show when={!["complete", "canceled"].includes(g().status)}>
                  <button
                    class="pressable px-2 py-0.5 rounded text-2xs border border-[var(--border)] text-[var(--err)] disabled:opacity-50"
                    disabled={acting()}
                    onClick={() => act("cancel")}
                  >
                    取消
                  </button>
                </Show>
              </div>
            </div>
          )}
        </Show>
      </Section>

      <Section title="改动" icon={FileDiff}>
        <Show
          when={changes().length > 0}
          fallback={<div class="text-xs text-[var(--text-faint)]">本会话暂无 agent 改动</div>}
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
                      class="font-mono text-2xs w-10 shrink-0"
                      classList={{
                        "text-[var(--ok)]": c.status === "created",
                        "text-[var(--warn)]": c.status === "modified",
                        "text-[var(--err)]": c.status === "deleted",
                      }}
                    >
                      {c.status === "created" ? "新增" : c.status === "deleted" ? "删除" : "修改"}
                    </span>
                    <span class="truncate font-mono text-[var(--text-dim)] flex-1">{c.path}</span>
                    <span class="text-2xs tabular-nums shrink-0">
                      <span class="text-[var(--ok)]">+{c.added}</span>{" "}
                      <span class="text-[var(--err)]">-{c.deleted}</span>
                    </span>
                  </button>
                  <Show when={openDiff()?.path === c.path}>
                    <div class="mt-1 mb-2 text-2xs max-h-72 overflow-auto rounded border border-[var(--border)]">
                      <Markdown text={"```diff\n" + (openDiff()?.text ?? "") + "\n```"} />
                    </div>
                  </Show>
                </div>
              )}
            </For>
          </div>
        </Show>
      </Section>

      <Section title="后台任务" icon={SquareTerminal}>
        <Show
          when={tasks().length > 0}
          fallback={<div class="text-xs text-[var(--text-faint)]">无后台任务</div>}
        >
          <div class="space-y-1.5">
            <For each={tasks()}>
              {(t) => (
                <div class="text-xs space-y-0.5">
                  <div class="flex items-center gap-1.5">
                    <span class={`text-2xs font-medium ${TASK_STATUS[t.status] ?? ""}`}>
                      {t.status}
                    </span>
                    <Show when={t.port}>
                      <a
                        class="text-2xs text-[var(--accent-hover)]"
                        href={`http://localhost:${t.port}`}
                        target="_blank"
                        rel="noreferrer"
                      >
                        :{t.port}
                      </a>
                    </Show>
                    <Show when={t.status === "running"}>
                      <button
                        class="pressable ml-auto px-1.5 py-0 rounded text-2xs border border-[var(--border)] text-[var(--err)]"
                        onClick={() => void taskKill(t.id).then(reloadTasks)}
                      >
                        终止
                      </button>
                    </Show>
                  </div>
                  <div
                    class="font-mono text-2xs text-[var(--text-dim)] truncate cursor-pointer hover:text-[var(--text)]"
                    title={t.command}
                    onClick={() => setOpenTask(openTask() === t.id ? "" : t.id)}
                  >
                    {t.command}
                  </div>
                  <Show when={openTask() === t.id && t.tail}>
                    <pre class="max-h-32 overflow-auto rounded border border-[var(--border)] bg-[var(--bg)] p-1.5 text-2xs font-mono text-[var(--text-dim)] whitespace-pre-wrap">
                      {t.tail}
                    </pre>
                  </Show>
                </div>
              )}
            </For>
          </div>
        </Show>
      </Section>
    </>
  );
}

export default function Dock() {
  const [goal, setGoal] = createSignal<GoalInfo | null>(null);
  const [changes, setChanges] = createSignal<AgentDiffEntry[]>([]);
  const [tasks, setTasks] = createSignal<TaskInfo[]>([]);
  const [openDiff, setOpenDiff] = createSignal<{ path: string; text: string } | null>(null);
  let unlisten: (() => void) | undefined;
  let offResync: (() => void) | undefined;
  let timer: ReturnType<typeof setInterval> | undefined;

  // goalAction：act 期间禁用按钮（连点产生并发 transit 裸 rejection 的根因），失败走 flashErr
  const goalAction = createAction();

  // 焦点带会话口径（与 StatusBar 一致）；焦点为空回落最近更新的 goal，complete/canceled 终态也有呈现
  const reloadGoal = async () => {
    try {
      const focused = await goalFocus(activeSessionId() || undefined);
      setGoal(focused ?? (await goalList())[0] ?? null);
    } catch {
      // 事件/轮询驱动：本轮失败保留旧值，下一轮重拉
    }
  };
  const reloadDiff = async () => {
    const sid = activeSessionId();
    setChanges(sid ? await agentDiffStatus(sid) : []);
  };
  const reloadTasks = async () => setTasks(await taskList());

  onMount(async () => {
    await Promise.all([reloadGoal(), reloadDiff(), reloadTasks()]);
    unlisten = await onTopic(["goal.update", "task.update"], () => {
      void reloadGoal();
      void reloadTasks();
    });
    // goal.update/task.update 丢帧后 topic 流不自愈：resync 信号按真源重拉
    offResync = client.onResync(() => {
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
    offResync?.();
    if (timer) clearInterval(timer);
  });

  const act = (action: "activate" | "pause" | "resume" | "cancel") => {
    const g = goal();
    if (!g) return;
    void goalAction.run(
      async () => {
        await goalTransit(g.id, action);
        await reloadGoal();
      },
      { errPrefix: "goal 操作失败" },
    );
  };

  const toggleDiff = async (path: string) => {
    if (openDiff()?.path === path) {
      setOpenDiff(null);
      return;
    }
    const text = await agentDiffFile(activeSessionId(), path);
    setOpenDiff({ path, text });
  };

  const badge = () => GOAL_STATUS[goal()?.status ?? ""] ?? { text: "", cls: "" };

  return (
    <aside class="w-full h-full overflow-y-auto">
      <DockSections
        goal={goal()}
        badge={badge}
        act={act}
        acting={goalAction.pending}
        changes={changes()}
        openDiff={openDiff()}
        toggleDiff={toggleDiff}
        tasks={tasks()}
        reloadTasks={() => void reloadTasks()}
      />
      <DockWorktree />
    </aside>
  );
}
