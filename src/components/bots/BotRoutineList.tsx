import { For, Show } from "solid-js";
import type { BotRoutine } from "../../lib/bots";
import { actionClass, Panel, shortId, statusClass } from "./shared";

type RoutineMutation = "pause" | "resume" | "run" | "trash";

interface Props {
  routines: BotRoutine[];
  acting: boolean;
  loadError: string;
  onEdit: (routine: BotRoutine) => void;
  onMutate: (routine: BotRoutine, kind: RoutineMutation) => void;
}

export default function BotRoutineList(props: Props) {
  return (
    <div class="lg:col-span-2">
      <Panel
        title="Routines"
        detail="IANA timezone、misfire policy、去重 occurrence 和失败自动暂停均由后端持久化执行。"
      >
        <Show when={props.loadError}>
          <p class="text-xs text-[var(--err)] mb-2">{props.loadError}</p>
        </Show>
        <div class="space-y-3">
          <For
            each={props.routines}
            fallback={<p class="text-xs text-[var(--text-faint)]">暂无 Routine。</p>}
          >
            {(routine) => (
              <div class="rounded border border-[var(--border)] p-3">
                <div class="flex items-center gap-2">
                  <span class="text-sm">{routine.definition.name}</span>
                  <span class={`text-2xs ${statusClass(routine.lifecycle)}`}>
                    {routine.lifecycle}
                  </span>
                  <span class="ml-auto text-2xs font-mono text-[var(--text-faint)]">
                    {shortId(routine.routine_id)}
                  </span>
                </div>
                <div class="grid grid-cols-2 gap-2 mt-2 text-xs text-[var(--text-dim)]">
                  <div>Bot: {routine.definition.bot_id}</div>
                  <div>Context: {routine.definition.context_mode}</div>
                  <div>
                    Cron:{" "}
                    {routine.definition.schedule.expression.kind === "cron"
                      ? routine.definition.schedule.expression.expression
                      : "once"}
                  </div>
                  <div>Timezone: {routine.definition.schedule.timezone}</div>
                  <div>
                    Next:{" "}
                    {routine.next_scheduled_at_ms
                      ? new Date(routine.next_scheduled_at_ms).toLocaleString()
                      : "none"}
                  </div>
                  <div>
                    Failures: {routine.consecutive_failures}/{routine.definition.failure_threshold}
                  </div>
                </div>
                <div class="flex flex-wrap gap-2 mt-3">
                  <button
                    class={actionClass}
                    disabled={props.acting}
                    onClick={() => props.onEdit(routine)}
                  >
                    编辑
                  </button>
                  <Show when={routine.lifecycle === "active"}>
                    <button
                      class={actionClass}
                      disabled={props.acting}
                      onClick={() => props.onMutate(routine, "run")}
                    >
                      Run now
                    </button>
                    <button
                      class={actionClass}
                      disabled={props.acting}
                      onClick={() => props.onMutate(routine, "pause")}
                    >
                      Pause
                    </button>
                  </Show>
                  <Show when={routine.lifecycle === "paused"}>
                    <button
                      class={actionClass}
                      disabled={props.acting}
                      onClick={() => props.onMutate(routine, "resume")}
                    >
                      Resume
                    </button>
                  </Show>
                  <Show when={routine.lifecycle !== "trashed"}>
                    <button
                      class={actionClass}
                      disabled={props.acting}
                      onClick={() => props.onMutate(routine, "trash")}
                    >
                      Trash
                    </button>
                  </Show>
                </div>
                <Show when={Object.values(routine.occurrences).length > 0}>
                  <div class="mt-3 border-t border-[var(--border)] pt-2 space-y-1">
                    <For each={Object.values(routine.occurrences).slice(-5)}>
                      {(occurrence) => (
                        <div class="text-2xs flex gap-2">
                          <span class={statusClass(occurrence.status)}>{occurrence.status}</span>
                          <span>{occurrence.manual ? "manual" : "scheduled"}</span>
                          <span class="font-mono">
                            {shortId(occurrence.run_id || occurrence.occurrence_id)}
                          </span>
                          <Show when={occurrence.error}>
                            <span class="text-[var(--err)]">{occurrence.error}</span>
                          </Show>
                        </div>
                      )}
                    </For>
                  </div>
                </Show>
              </div>
            )}
          </For>
        </div>
      </Panel>
    </div>
  );
}
