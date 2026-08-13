import { createEffect, createSignal } from "solid-js";
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
import { createReconciledMutation } from "../../lib/async-guard";
import { formatError } from "../../lib/error-text";
import { encodeBotInput, publishedBotDefinition } from "./bot-definition";
import { type RefreshProps } from "./shared";
import BotRoutineForm from "./BotRoutineForm";
import BotRoutineList from "./BotRoutineList";
import {
  editableRoutineCron,
  editableRoutineInput,
  routineDefinitionApplied,
  routineMutationApplied,
} from "./mutation-state";

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
  const mutation = createReconciledMutation({ refresh: reload, onChanged: props.onChanged });
  const acting = mutation.pending;
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
    const definitionKey = JSON.stringify(value);
    void mutation.run({
      key: current
        ? `routine:${current.routine_id}:update:${definitionKey}`
        : `routine:create:${definitionKey}`,
      prepare: () => ({
        routineId: current?.routine_id ?? newBotId("routine"),
        idempotencyKey: newBotId("idem"),
      }),
      execute: ({ routineId, idempotencyKey }) => {
        const persisted = routines().find((routine) => routine.routine_id === routineId);
        return current
          ? botRoutineUpdate(
              routineId,
              value,
              persisted?.event_version ?? current.event_version,
              idempotencyKey,
            )
          : botRoutineCreate(routineId, value, idempotencyKey);
      },
      applied: ({ routineId }) => routineDefinitionApplied(routines(), routineId, definitionKey),
      onApplied: reset,
      okText: current ? "Routine 已更新" : "Routine 已创建",
    });
  };
  const edit = (routine: BotRoutine) => {
    const definition = routine.definition;
    setEditing(routine);
    setBotId(definition.bot_id);
    setName(definition.name);
    setCron(editableRoutineCron(routine));
    setTimezone(definition.schedule.timezone);
    setInput(editableRoutineInput(routine));
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
    const routineId = routine.routine_id;
    void mutation.run({
      key: `routine:${routineId}:${kind}`,
      prepare: () => ({ occurrenceId: newBotId("occ"), idempotencyKey: newBotId("idem") }),
      execute: ({ occurrenceId, idempotencyKey }) => {
        const current = routines().find((item) => item.routine_id === routineId);
        if (!current) throw new Error("Routine state is unavailable");
        const jobs = {
          pause: () =>
            botRoutinePause(routineId, current.event_version, idempotencyKey, "paused by owner"),
          resume: () => botRoutineResume(routineId, current.event_version, idempotencyKey),
          run: () =>
            botRoutineRunNow(routineId, occurrenceId, current.event_version, idempotencyKey),
          trash: () => botRoutineTrash(routineId, current.event_version, idempotencyKey),
        };
        return jobs[kind]();
      },
      applied: ({ occurrenceId }) =>
        routineMutationApplied(routines(), routineId, kind, occurrenceId),
      okText: kind === "run" ? "Routine occurrence 已记录" : `Routine ${kind} 已提交`,
    });
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

      <BotRoutineList
        routines={routines()}
        acting={acting()}
        loadError={loadErr()}
        onEdit={edit}
        onMutate={mutate}
      />
    </div>
  );
}
