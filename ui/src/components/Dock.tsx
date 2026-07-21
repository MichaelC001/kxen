import { createSignal, For, Show, onCleanup, onMount } from "solid-js";
import {
  diffFile,
  diffStatus,
  goalFocus,
  goalTransit,
  onTopic,
  taskKill,
  taskList,
  teamList,
  teamMessage,
  type DiffStatusEntry,
  type GoalInfo,
  type TaskInfo,
  type TeamMember,
  type TeamTask,
} from "../lib/chat";
import { activeSessionId } from "../lib/state";
import Markdown from "./Markdown";
import { ArrowLeft, FileDiff, SquareTerminal, Target, Users } from "lucide-solid";

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

const MEMBER_STATUS: Record<string, { text: string; cls: string }> = {
  working: { text: "工作中", cls: "bg-[var(--ok)]" },
  idle: { text: "空闲", cls: "bg-[var(--text-faint)]" },
  awaiting_plan_approval: { text: "待审批", cls: "bg-[var(--warn)]" },
  failed: { text: "失败", cls: "bg-[var(--err)]" },
  shutdown: { text: "已关闭", cls: "bg-[var(--text-faint)]" },
};

/** teammate 转录子视图：llm.delta 按 agent 过滤的实时流 + 直发输入框。 */
function TeammateView(props: { name: string; onBack: () => void }) {
  const [lines, setLines] = createSignal<Array<{ kind: string; text: string }>>([]);
  const [draft, setDraft] = createSignal("");
  let unlisten: (() => void) | undefined;

  onMount(async () => {
    unlisten = await onTopic(["llm.delta"], (_topic, payload) => {
      const p = payload as {
        agent?: string;
        session_id?: string;
        kind?: string;
        text?: string;
        name?: string;
        summary?: string;
      };
      if (p.agent !== props.name || p.session_id !== activeSessionId()) return;
      if (p.kind === "text" && p.text) {
        setLines((prev) => {
          const last = prev[prev.length - 1];
          if (last?.kind === "text")
            return [...prev.slice(0, -1), { kind: "text", text: last.text + p.text }];
          return [...prev, { kind: "text", text: p.text! }];
        });
      } else if (p.kind === "tool_call") {
        setLines((prev) => [...prev, { kind: "tool", text: `${p.name}: ${p.summary ?? ""}` }]);
      } else if (p.kind === "error") {
        setLines((prev) => [...prev, { kind: "error", text: p.summary ?? p.text ?? "error" }]);
      }
    });
  });
  onCleanup(() => unlisten?.());

  const send = async () => {
    const text = draft().trim();
    if (!text) return;
    setDraft("");
    setLines((prev) => [...prev, { kind: "me", text }]);
    await teamMessage(activeSessionId(), props.name, text);
  };

  return (
    <div class="h-full flex flex-col">
      <div class="px-3 py-2 border-b border-[var(--border)] flex items-center gap-2">
        <button
          class="pressable p-1 rounded text-[var(--text-dim)] hover:bg-[var(--bg-overlay)]/60"
          onClick={props.onBack}
        >
          <ArrowLeft size={13} />
        </button>
        <span class="text-xs font-medium">{props.name}</span>
      </div>
      <div class="flex-1 overflow-auto px-3 py-2 space-y-1.5 text-xs">
        <For each={lines()}>
          {(line) => (
            <div
              classList={{
                "text-[var(--text)]": line.kind === "text",
                "text-[var(--text-faint)] font-mono text-[10px]": line.kind === "tool",
                "text-[var(--err)]": line.kind === "error",
                "text-[var(--accent-hover)]": line.kind === "me",
              }}
            >
              {line.kind === "me" ? `-> ${line.text}` : line.text}
            </div>
          )}
        </For>
        <Show when={lines().length === 0}>
          <div class="text-[var(--text-faint)]">暂无转录（teammate 的输出会实时出现在这里）</div>
        </Show>
      </div>
      <div class="p-2 border-t border-[var(--border)] flex gap-1.5">
        <input
          class="flex-1 bg-transparent border border-[var(--border)] rounded px-2 py-1 text-xs"
          placeholder={`对 ${props.name} 说话…`}
          value={draft()}
          onInput={(e) => setDraft(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void send();
          }}
        />
      </div>
    </div>
  );
}

function Section(props: { title: string; icon: unknown; children: unknown }) {
  const Icon = props.icon as (p: { size: number; class?: string }) => unknown;
  return (
    <div class="border-b border-[var(--border)] px-3 py-3">
      <div class="text-[10px] uppercase tracking-wider text-[var(--text-faint)] mb-2 flex items-center gap-1.5">
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
  changes: DiffStatusEntry[];
  openDiff: { path: string; text: string } | null;
  toggleDiff: (path: string) => void;
  tasks: TaskInfo[];
  members: TeamMember[];
  teamTasks: TeamTask[];
  onSelectMember: (name: string) => void;
}) {
  const goal = () => props.goal;
  const badge = props.badge;
  const act = props.act;
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

      <Section title="改动" icon={FileDiff}>
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

      <Section title="团队" icon={Users}>
        <Show
          when={props.members.length > 0}
          fallback={
            <div class="text-xs text-[var(--text-faint)]">
              无 teammates。让主代理 spawn 一个团队试试。
            </div>
          }
        >
          <div class="space-y-1">
            <For each={props.members}>
              {(m) => (
                <button
                  class="w-full flex items-center gap-1.5 px-1 py-0.5 rounded text-xs text-left hover:bg-[var(--bg-overlay)]/60"
                  onClick={() => props.onSelectMember(m.name)}
                >
                  <span
                    class={`w-1.5 h-1.5 rounded-full shrink-0 ${MEMBER_STATUS[m.status]?.cls ?? "bg-[var(--text-faint)]"}`}
                  />
                  <span class="font-medium">{m.name}</span>
                  <span class="text-[10px] text-[var(--text-faint)] truncate">{m.model.model}</span>
                  <span class="text-[10px] text-[var(--text-faint)] ml-auto">
                    {MEMBER_STATUS[m.status]?.text ?? m.status}
                  </span>
                </button>
              )}
            </For>
            <For each={props.teamTasks}>
              {(t) => (
                <div class="flex items-center gap-1.5 px-1 text-[10px] text-[var(--text-dim)]">
                  <span class="font-mono">#{t.id}</span>
                  <span class="truncate flex-1">{t.title}</span>
                  <span
                    classList={{
                      "text-[var(--ok)]": t.status === "completed",
                      "text-[var(--accent-hover)]": t.status === "in_progress",
                    }}
                  >
                    {t.status === "completed"
                      ? "done"
                      : t.status === "in_progress"
                        ? `${t.assignee ?? ""}`
                        : "pending"}
                  </span>
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
  const [changes, setChanges] = createSignal<DiffStatusEntry[]>([]);
  const [tasks, setTasks] = createSignal<TaskInfo[]>([]);
  const [openDiff, setOpenDiff] = createSignal<{ path: string; text: string } | null>(null);
  const [members, setMembers] = createSignal<TeamMember[]>([]);
  const [teamTasks, setTeamTasks] = createSignal<TeamTask[]>([]);
  const [selectedMember, setSelectedMember] = createSignal<string | null>(null);
  let unlisten: (() => void) | undefined;
  let timer: ReturnType<typeof setInterval> | undefined;

  const reloadGoal = async () => setGoal(await goalFocus());
  const reloadDiff = async () => setChanges(await diffStatus().catch(() => []));
  const reloadTasks = async () => setTasks(await taskList());
  const reloadTeam = async () => {
    const sid = activeSessionId();
    if (!sid) {
      setMembers([]);
      setTeamTasks([]);
      return;
    }
    const data = await teamList(sid).catch(() => null);
    if (data) {
      setMembers(data.members);
      setTeamTasks(data.tasks);
    }
  };

  onMount(async () => {
    await Promise.all([reloadGoal(), reloadDiff(), reloadTasks(), reloadTeam()]);
    unlisten = await onTopic(["goal.update", "task.update"], () => {
      void reloadGoal();
      void reloadTasks();
      void reloadTeam();
    });
    timer = setInterval(() => {
      void reloadDiff();
      void reloadTasks();
      void reloadTeam();
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
    <aside class="w-full h-full border-l border-[var(--border)] bg-[var(--bg-raised)] overflow-y-auto">
      <Show
        when={selectedMember()}
        fallback={
          <DockSections
            goal={goal()}
            badge={badge()}
            act={act}
            changes={changes()}
            openDiff={openDiff()}
            toggleDiff={toggleDiff}
            tasks={tasks()}
            members={members()}
            teamTasks={teamTasks()}
            onSelectMember={setSelectedMember}
          />
        }
      >
        {(name) => <TeammateView name={name()} onBack={() => setSelectedMember(null)} />}
      </Show>
    </aside>
  );
}
