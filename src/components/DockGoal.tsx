// Dock 目标分区：goal 徽标 / 判据 / 验证证据 / 操作按钮（自 Dock.tsx 拆出，350 行门禁）。
// budget_limited 不给裸「恢复」：已用量 >= 限额不变，下一轮立刻再超限，只留「提高预算并继续」。
import { Show } from "solid-js";
import { Target } from "lucide-solid";
import type { GoalAction, GoalInfo } from "../lib/chat";
import { insertComposerText } from "../lib/composer-bus";
import DockSection from "./DockSection";

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

export default function DockGoal(props: {
  goal: GoalInfo | null;
  act: (action: GoalAction) => void;
  acting: () => boolean;
}) {
  const goal = () => props.goal;
  const act = props.act;
  const acting = props.acting;
  const badge = () => GOAL_STATUS[goal()?.status ?? ""] ?? { text: "", cls: "" };
  return (
    <DockSection title="目标" icon={Target}>
      <Show
        when={goal()}
        fallback={
          <div class="text-xs text-[var(--text-faint)]">
            无焦点 goal。
            <button
              class="text-[var(--accent-hover)] hover:underline"
              title="填入 composer，回车发送"
              onClick={() => insertComposerText("/write-goal ")}
            >
              填入 /write-goal 创建
            </button>
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
              <Show when={["paused", "blocked"].includes(g().status)}>
                <button
                  class="pressable px-2 py-0.5 rounded text-2xs bg-[var(--accent)] text-white disabled:opacity-50"
                  disabled={acting()}
                  onClick={() => act("resume")}
                >
                  恢复
                </button>
              </Show>
              <Show when={g().status === "budget_limited"}>
                <button
                  class="pressable px-2 py-0.5 rounded text-2xs bg-[var(--accent)] text-white disabled:opacity-50"
                  disabled={acting()}
                  onClick={() => act("adjust")}
                >
                  提高预算并继续
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
    </DockSection>
  );
}
