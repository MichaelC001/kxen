import { Show, createEffect, createSignal, untrack } from "solid-js";
import {
  botBuilderGet,
  botBuilderList,
  botBuilderMessage,
  botBuilderStart,
  newBotId,
  type BuilderState,
} from "../../lib/bots";
import { createReconciledMutation } from "../../lib/async-guard";
import { formatError } from "../../lib/error-text";
import { Panel, type BotBuilderTarget, type RefreshProps } from "./shared";
import BotBuilderConversation from "./BotBuilderConversation";
import BotBuilderReview from "./BotBuilderReview";
import BotBuilderStart from "./BotBuilderStart";

interface PendingStart {
  botId: string;
  sessionId: string;
  goal: string;
  name: string;
  startKey: string;
  messageId: string;
  messageKey: string;
}

interface PendingTurn {
  builderId: string;
  messageId: string;
  text: string;
  idempotencyKey: string;
}

export default function BotBuilder(
  props: RefreshProps & {
    target?: BotBuilderTarget | undefined;
    onClearTarget?: (() => void) | undefined;
  },
) {
  const [builder, setBuilder] = createSignal<BuilderState | null>(null);
  const [builderId, setBuilderId] = createSignal("");
  const [name, setName] = createSignal("");
  const [goal, setGoal] = createSignal("");
  const [message, setMessage] = createSignal("");
  const [loadErr, setLoadErr] = createSignal("");
  const [targetLoading, setTargetLoading] = createSignal(false);
  const [pendingStart, setPendingStart] = createSignal<PendingStart | null>(null);
  const [pendingTurn, setPendingTurn] = createSignal<PendingTurn | null>(null);
  let loadSeq = 0;
  let loadedTargetId = "";

  const reload = async () => {
    const id = builderId().trim();
    if (!id) return false;
    const seq = ++loadSeq;
    if (untrack(builder)?.builder_session_id !== id) setBuilder(null);
    try {
      const state = await botBuilderGet(id);
      if (seq !== loadSeq) return;
      setBuilder(state);
      setLoadErr("");
      return true;
    } catch (error) {
      if (seq === loadSeq) setLoadErr(formatError(error));
      return false;
    }
  };
  createEffect(() => {
    void props.epoch;
    if (builderId()) void reload();
  });
  createEffect(() => {
    const target = props.target;
    if (!target || target.bot_id === loadedTargetId) return;
    loadedTargetId = target.bot_id;
    setName(target.display_name);
    setGoal("");
    setMessage("");
    setBuilder(null);
    setBuilderId("");
    setPendingStart(null);
    setPendingTurn(null);
    setLoadErr("");
    const seq = ++loadSeq;
    setTargetLoading(true);
    void botBuilderList(target.bot_id)
      .then((sessions) => {
        if (seq !== loadSeq) return;
        const active = sessions.find((session) => session.lifecycle === "active");
        if (active) {
          setBuilderId(active.builder_session_id);
          setBuilder(active);
        }
      })
      .catch((error: unknown) => {
        if (seq === loadSeq) setLoadErr(formatError(error));
      })
      .finally(() => {
        if (seq === loadSeq) setTargetLoading(false);
      });
  });

  const mutation = createReconciledMutation({ refresh: reload, onChanged: props.onChanged });
  const acting = mutation.pending;
  const start = () => {
    const existing = pendingStart();
    const cleanGoal = goal().trim();
    const cleanName = name().trim();
    if (!existing && (!cleanGoal || !cleanName)) return;
    const operation: PendingStart = existing ?? {
      botId: props.target?.bot_id ?? newBotId("bot"),
      sessionId: newBotId("builder"),
      goal: cleanGoal,
      name: cleanName,
      startKey: newBotId("idem"),
      messageId: newBotId("bmessage"),
      messageKey: newBotId("idem"),
    };
    setPendingStart(operation);
    setBuilderId(operation.sessionId);
    void mutation.run({
      key: `builder:${operation.sessionId}:start`,
      prepare: () => operation,
      execute: async () => {
        await botBuilderStart(
          operation.botId,
          operation.sessionId,
          operation.goal,
          operation.name,
          operation.startKey,
        );
        const state = await botBuilderMessage(
          operation.sessionId,
          operation.messageId,
          operation.goal,
          operation.messageKey,
        );
        setBuilder(state);
      },
      applied: () => hasBuilderReply(builder(), operation.messageId),
      onApplied: () => {
        setPendingStart(null);
        setGoal("");
      },
      okText: "Builder 已回复",
    });
  };
  const send = () => {
    if (pendingStart()) {
      start();
      return;
    }
    const id = builderId();
    const remotePending = pendingOwnerMessage(builder());
    const existing = pendingTurn();
    const text = message().trim();
    if (!id || (!remotePending && !existing && !text)) return;
    const operation: PendingTurn =
      existing ??
      (remotePending
        ? {
            builderId: id,
            messageId: remotePending.message_id,
            text: remotePending.text,
            idempotencyKey: newBotId("idem"),
          }
        : {
            builderId: id,
            messageId: newBotId("bmessage"),
            text,
            idempotencyKey: newBotId("idem"),
          });
    setPendingTurn(operation);
    void mutation.run({
      key: `builder:${operation.builderId}:message:${operation.messageId}`,
      prepare: () => operation,
      execute: async () => {
        const state = await botBuilderMessage(
          operation.builderId,
          operation.messageId,
          operation.text,
          operation.idempotencyKey,
        );
        setBuilder(state);
      },
      applied: () => hasBuilderReply(builder(), operation.messageId),
      onApplied: () => {
        setPendingTurn(null);
        setMessage("");
      },
      okText: "Builder 已回复",
    });
  };
  const clearTarget = () => {
    if (acting()) return;
    loadedTargetId = "";
    ++loadSeq;
    setBuilder(null);
    setBuilderId("");
    setName("");
    setGoal("");
    setMessage("");
    setPendingStart(null);
    setPendingTurn(null);
    setLoadErr("");
    setTargetLoading(false);
    props.onClearTarget?.();
  };
  return (
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-4">
      <BotBuilderStart
        name={name()}
        goal={goal()}
        builderId={builderId()}
        acting={acting()}
        loadErr={loadErr()}
        target={props.target}
        targetLoading={targetLoading()}
        retryingStart={Boolean(pendingStart())}
        setName={setName}
        setGoal={setGoal}
        setBuilderId={setBuilderId}
        start={start}
        reload={() => void reload()}
        clearTarget={clearTarget}
      />

      <div class="lg:col-span-2 space-y-4">
        <Show
          when={builder()}
          fallback={
            <Panel title="Build Workspace">
              <p class="text-xs text-[var(--text-faint)]">
                创建或加载一个 Builder Session 后开始。
              </p>
            </Panel>
          }
        >
          {(state) => (
            <>
              <BotBuilderConversation
                builder={state()}
                message={message()}
                acting={acting()}
                retrying={Boolean(pendingTurn() || pendingOwnerMessage(state()))}
                setMessage={setMessage}
                send={send}
              />
              <BotBuilderReview state={state()} mutation={mutation} />
            </>
          )}
        </Show>
      </div>
    </div>
  );
}

function pendingOwnerMessage(state: BuilderState | null) {
  const last = state?.messages.at(-1);
  return last?.actor.kind === "owner" ? last : undefined;
}

function hasBuilderReply(state: BuilderState | null, sourceMessageId: string): boolean {
  const sourceIndex =
    state?.messages.findIndex((item) => item.message_id === sourceMessageId) ?? -1;
  return Boolean(
    state &&
    sourceIndex >= 0 &&
    state.messages
      .slice(sourceIndex + 1)
      .some((item) => item.actor.kind === "system" && item.actor.actor === "builder"),
  );
}
