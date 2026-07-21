import { createSignal, For, Show, onMount, onCleanup } from "solid-js";
import { goalFocus, goalList, goalTransit, onTopic, type GoalInfo } from "../lib/chat";

const STATUS_LABEL: Record<string, { text: string; cls: string }> = {
  draft: { text: "草稿", cls: "text-[var(--text-dim)]" },
  queued: { text: "排队", cls: "text-[var(--text-dim)]" },
  active: { text: "进行中", cls: "text-[var(--accent-hover)]" },
  paused: { text: "已暂停", cls: "text-[var(--warn)]" },
  blocked: { text: "阻塞", cls: "text-[var(--err)]" },
  budgetlimited: { text: "预算耗尽", cls: "text-[var(--err)]" },
  complete: { text: "已完成", cls: "text-[var(--ok)]" },
  canceled: { text: "已取消", cls: "text-[var(--text-faint)]" },
};

function GoalCard(props: { goal: GoalInfo; focused: boolean; onAction: () => void }) {
  const g = () => props.goal;
  const badge = () => STATUS_LABEL[g().status] ?? { text: g().status, cls: "" };
  const act = async (action: "activate" | "pause" | "resume" | "cancel") => {
    await goalTransit(g().id, action);
    props.onAction();
  };

  return (
    <div
      class="rounded-lg border p-4 space-y-2"
      classList={{
        "border-[var(--accent)] bg-[var(--bg-raised)]": props.focused,
        "border-[var(--border)] bg-[var(--bg-raised)]": !props.focused,
      }}
    >
      <div class="flex items-center gap-2">
        <span class={`text-xs font-medium ${badge().cls}`}>{badge().text}</span>
        <Show when={props.focused}>
          <span class="text-[10px] px-1.5 py-0.5 rounded bg-[var(--accent)]/20 text-[var(--accent-hover)]">
            焦点
          </span>
        </Show>
        <span class="text-[10px] text-[var(--text-faint)] ml-auto font-mono">{g().id}</span>
      </div>
      <div class="text-sm">{g().objective}</div>
      <div class="text-xs text-[var(--text-dim)]">
        <span class="text-[var(--text-faint)]">完成判据：</span>
        {g().completion_criteria}
      </div>
      <Show when={g().constraints}>
        <div class="text-xs text-[var(--text-dim)]">
          <span class="text-[var(--text-faint)]">约束：</span>
          {g().constraints}
        </div>
      </Show>
      <div class="text-[10px] text-[var(--text-faint)]">
        turns {g().turns_used}
        {g().budget.turns ? `/${g().budget.turns}` : ""} · tokens {g().tokens_used}
        {g().budget.tokens ? `/${g().budget.tokens}` : ""} · blocks {g().consecutive_blocks}
      </div>
      <Show when={g().block_reason}>
        <div class="text-xs text-[var(--err)]">阻塞原因：{g().block_reason}</div>
      </Show>
      <Show when={g().verification_evidence}>
        <div class="text-xs text-[var(--ok)]">证据：{g().verification_evidence}</div>
      </Show>
      <div class="flex gap-2 pt-1">
        <Show
          when={g().status === "draft" || g().status === "queued" || g().status === "budgetlimited"}
        >
          <button
            class="pressable px-2.5 py-1 rounded text-xs bg-[var(--accent)] hover:bg-[var(--accent-hover)] text-white"
            onClick={() => void act("activate")}
          >
            激活
          </button>
        </Show>
        <Show when={g().status === "active"}>
          <button
            class="pressable px-2.5 py-1 rounded text-xs border border-[var(--border)] text-[var(--warn)]"
            onClick={() => void act("pause")}
          >
            暂停
          </button>
        </Show>
        <Show when={g().status === "paused" || g().status === "blocked"}>
          <button
            class="pressable px-2.5 py-1 rounded text-xs bg-[var(--accent)] hover:bg-[var(--accent-hover)] text-white"
            onClick={() => void act("resume")}
          >
            恢复
          </button>
        </Show>
        <Show
          when={["draft", "queued", "active", "paused", "blocked", "budgetlimited"].includes(
            g().status,
          )}
        >
          <button
            class="pressable px-2.5 py-1 rounded text-xs border border-[var(--border)] text-[var(--err)]"
            onClick={() => void act("cancel")}
          >
            取消
          </button>
        </Show>
      </div>
    </div>
  );
}

export default function Goals() {
  const [goals, setGoals] = createSignal<GoalInfo[]>([]);
  const [focusId, setFocusId] = createSignal<string | null>(null);
  let unlisten: (() => void) | undefined;

  const reload = async () => {
    const [list, focus] = await Promise.all([goalList(), goalFocus()]);
    setGoals(list);
    setFocusId(focus?.id ?? null);
  };

  onMount(async () => {
    await reload();
    unlisten = await onTopic(["goal.update"], () => void reload());
  });
  onCleanup(() => unlisten?.());

  return (
    <div class="h-full overflow-auto p-6">
      <div class="max-w-2xl mx-auto space-y-4">
        <div class="flex items-center justify-between">
          <h1 class="text-lg font-semibold">目标</h1>
          <span class="text-xs text-[var(--text-faint)]">在会话里说 write-goal 创建新目标</span>
        </div>
        <Show when={goals().length === 0}>
          <div class="text-sm text-[var(--text-faint)] text-center mt-16">
            还没有 goal。到会话里用 write-goal 定义一个带完成判据的目标。
          </div>
        </Show>
        <For each={goals()}>
          {(g) => <GoalCard goal={g} focused={g.id === focusId()} onAction={() => void reload()} />}
        </For>
      </div>
    </div>
  );
}
