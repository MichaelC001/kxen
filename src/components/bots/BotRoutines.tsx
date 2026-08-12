import { For, Show, createEffect, createSignal } from "solid-js";
import {
  botConversationList,
  botGet,
  botList,
  botRoutineCreate,
  botRoutineList,
  botRoutinePause,
  botRoutineResume,
  botRoutineRunNow,
  botRoutineTrash,
  botRoutineUpdate,
  newBotId,
  type BotConversation,
  type BotRoutine,
  type BotState,
  type BotSummary,
  type RoutineDefinition,
} from "../../lib/bots";
import { flashErr, flashOk } from "../../lib/flash";
import { formatError } from "../../lib/error-text";
import { encodeBotInput, publishedBotDefinition } from "./bot-definition";
import { actionClass, Panel, shortId, statusClass, type RefreshProps } from "./shared";
import BotRoutineForm from "./BotRoutineForm";

export default function BotRoutines(props: RefreshProps) {
  const [routines, setRoutines] = createSignal<BotRoutine[]>([]);
  const [bots, setBots] = createSignal<BotSummary[]>([]);
  const [conversations, setConversations] = createSignal<BotConversation[]>([]);
  const [editing, setEditing] = createSignal<BotRoutine | null>(null);
  const [botId, setBotId] = createSignal("");
  const [name, setName] = createSignal("");
  const [cron, setCron] = createSignal("0 9 * * *");
  const [timezone, setTimezone] = createSignal(
    Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC",
  );
  const [input, setInput] = createSignal("");
  const [contextMode, setContextMode] = createSignal<"isolated" | "continue_conversation">(
    "isolated",
  );
  const [conversationId, setConversationId] = createSignal("");
  const [revisionMode, setRevisionMode] = createSignal<"follow_current" | "pinned">(
    "follow_current",
  );
  const [failureThreshold, setFailureThreshold] = createSignal(3);
  const [selectedBot, setSelectedBot] = createSignal<BotState | null>(null);
  const [acting, setActing] = createSignal(false);
  const [loadErr, setLoadErr] = createSignal("");
  let loadSeq = 0;

  const reload = async () => {
    const seq = ++loadSeq;
    try {
      const [routineItems, botItems, conversationItems] = await Promise.all([
        botRoutineList(),
        botList(),
        botConversationList(undefined, false),
      ]);
      if (seq !== loadSeq) return;
      setRoutines(routineItems);
      setBots(botItems.filter((bot) => bot.lifecycle === "active"));
      setConversations(
        conversationItems.filter((conversation) => conversation.lifecycle === "active"),
      );
      setLoadErr("");
    } catch (error) {
      if (seq === loadSeq) setLoadErr(formatError(error));
    }
  };
  createEffect(() => {
    void props.epoch;
    void reload();
  });
  let botLoadSeq = 0;
  createEffect(() => {
    const id = botId();
    const seq = ++botLoadSeq;
    if (!id) {
      setSelectedBot(null);
      return;
    }
    setSelectedBot(null);
    void botGet(id)
      .then((state) => {
        if (seq === botLoadSeq) setSelectedBot(state);
      })
      .catch(() => {
        if (seq === botLoadSeq) setSelectedBot(null);
      });
  });
  const act = async (job: () => Promise<unknown>, label: string) => {
    if (acting()) return;
    setActing(true);
    try {
      await job();
      await reload();
      props.onChanged();
      flashOk(label);
    } catch (error) {
      flashErr(`${label}失败：${formatError(error)}`);
    } finally {
      setActing(false);
    }
  };
  const definition = (): RoutineDefinition | null => {
    const text = input().trim();
    const summary = bots().find((bot) => bot.bot_id === botId());
    const state = selectedBot();
    if (
      !summary ||
      !state ||
      state.bot_id !== summary.bot_id ||
      !name().trim() ||
      !cron().trim() ||
      !text
    )
      return null;
    if (contextMode() === "continue_conversation" && !conversationId()) return null;
    if (revisionMode() === "pinned" && !summary.current_revision_id) return null;
    let routineInput: RoutineDefinition["input"];
    try {
      routineInput = encodeBotInput(text, publishedBotDefinition(state));
    } catch {
      return null;
    }
    return {
      bot_id: summary.bot_id,
      name: name().trim(),
      schedule: {
        expression: { kind: "cron", expression: cron().trim() },
        timezone: timezone().trim(),
        misfire: "run_once",
        max_lateness_ms: 300000,
      },
      context_mode: contextMode(),
      ...(contextMode() === "continue_conversation"
        ? { target_conversation_id: conversationId() }
        : {}),
      input: routineInput,
      revision_policy:
        revisionMode() === "pinned"
          ? { kind: "pinned", revision_id: summary.current_revision_id! }
          : { kind: "follow_current" },
      failure_threshold: failureThreshold(),
    };
  };
  const save = () => {
    const value = definition();
    if (!value) return;
    const current = editing();
    void act(
      async () => {
        if (current)
          await botRoutineUpdate(
            current.routine_id,
            value,
            current.event_version,
            newBotId("idem"),
          );
        else await botRoutineCreate(newBotId("routine"), value, newBotId("idem"));
        reset();
      },
      current ? "Routine 已更新" : "Routine 已创建",
    );
  };
  const edit = (routine: BotRoutine) => {
    const definition = routine.definition;
    setEditing(routine);
    setBotId(definition.bot_id);
    setName(definition.name);
    setCron(
      definition.schedule.expression.kind === "cron"
        ? definition.schedule.expression.expression
        : "0 9 * * *",
    );
    setTimezone(definition.schedule.timezone);
    setInput(
      definition.input
        .map((part) => (part.kind === "text" ? part.text : JSON.stringify(part.fields, null, 2)))
        .join("\n"),
    );
    setContextMode(definition.context_mode);
    setConversationId(definition.target_conversation_id || "");
    setRevisionMode(definition.revision_policy.kind);
    setFailureThreshold(definition.failure_threshold);
  };
  const reset = () => {
    setEditing(null);
    setBotId("");
    setName("");
    setInput("");
    setConversationId("");
    setContextMode("isolated");
    setRevisionMode("follow_current");
  };
  const mutate = (routine: BotRoutine, kind: "pause" | "resume" | "run" | "trash") => {
    const key = newBotId("idem");
    const jobs = {
      pause: () =>
        botRoutinePause(routine.routine_id, routine.event_version, key, "paused by owner"),
      resume: () => botRoutineResume(routine.routine_id, routine.event_version, key),
      run: () => botRoutineRunNow(routine.routine_id, newBotId("occ"), routine.event_version, key),
      trash: () => botRoutineTrash(routine.routine_id, routine.event_version, key),
    };
    void act(jobs[kind], kind === "run" ? "Routine occurrence 已记录" : `Routine ${kind} 已提交`);
  };

  return (
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-4">
      <BotRoutineForm
        editing={editing()}
        bots={bots()}
        conversations={conversations()}
        botId={botId()}
        name={name()}
        cron={cron()}
        timezone={timezone()}
        input={input()}
        contextMode={contextMode()}
        conversationId={conversationId()}
        revisionMode={revisionMode()}
        failureThreshold={failureThreshold()}
        acting={acting()}
        valid={Boolean(definition())}
        setBotId={setBotId}
        setName={setName}
        setCron={setCron}
        setTimezone={setTimezone}
        setInput={setInput}
        setContextMode={setContextMode}
        setConversationId={setConversationId}
        setRevisionMode={setRevisionMode}
        setFailureThreshold={setFailureThreshold}
        save={save}
        reset={reset}
      />

      <div class="lg:col-span-2">
        <Panel
          title="Routines"
          detail="IANA timezone、misfire policy、去重 occurrence 和失败自动暂停均由后端持久化执行。"
        >
          <Show when={loadErr()}>
            <p class="text-xs text-[var(--err)] mb-2">{loadErr()}</p>
          </Show>
          <div class="space-y-3">
            <For
              each={routines()}
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
                      Failures: {routine.consecutive_failures}/
                      {routine.definition.failure_threshold}
                    </div>
                  </div>
                  <div class="flex flex-wrap gap-2 mt-3">
                    <button class={actionClass} disabled={acting()} onClick={() => edit(routine)}>
                      编辑
                    </button>
                    <Show when={routine.lifecycle === "active"}>
                      <button
                        class={actionClass}
                        disabled={acting()}
                        onClick={() => mutate(routine, "run")}
                      >
                        Run now
                      </button>
                      <button
                        class={actionClass}
                        disabled={acting()}
                        onClick={() => mutate(routine, "pause")}
                      >
                        Pause
                      </button>
                    </Show>
                    <Show when={routine.lifecycle === "paused"}>
                      <button
                        class={actionClass}
                        disabled={acting()}
                        onClick={() => mutate(routine, "resume")}
                      >
                        Resume
                      </button>
                    </Show>
                    <Show when={routine.lifecycle !== "trashed"}>
                      <button
                        class={actionClass}
                        disabled={acting()}
                        onClick={() => mutate(routine, "trash")}
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
    </div>
  );
}
