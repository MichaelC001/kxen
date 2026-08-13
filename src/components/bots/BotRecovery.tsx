import { For, Show, createEffect, createSignal } from "solid-js";
import {
  botRecoveryClear,
  botRecoveryInspect,
  botRecoveryRepair,
  newBotId,
  type BotRecoverySnapshot,
} from "../../lib/bots";
import { createReconciledMutation } from "../../lib/async-guard";
import { formatError } from "../../lib/error-text";
import { actionClass, Panel, shortId, statusClass, type RefreshProps } from "./shared";

export default function BotRecovery(props: RefreshProps) {
  const [snapshot, setSnapshot] = createSignal<BotRecoverySnapshot | null>(null);
  const [loadErr, setLoadErr] = createSignal("");
  let loadSeq = 0;
  const reload = async () => {
    const seq = ++loadSeq;
    try {
      const value = await botRecoveryInspect();
      if (seq !== loadSeq) return;
      setSnapshot(value);
      setLoadErr("");
    } catch (error) {
      if (seq === loadSeq) setLoadErr(formatError(error));
    }
  };
  createEffect(() => {
    void props.epoch;
    void reload();
  });
  const mutation = createReconciledMutation({ refresh: reload, onChanged: props.onChanged });
  const acting = mutation.pending;
  const resolve = (kind: "bot" | "bot_run", id: string, mode: "repair" | "clear") => {
    const version =
      kind === "bot"
        ? snapshot()?.bots.find((item) => item.bot_id === id)?.event_version
        : snapshot()?.runs.find((item) => item.spec.run_id === id)?.event_version;
    if (version === undefined || acting()) return;
    void mutation.run({
      key: `recovery:${kind}:${id}:${mode}`,
      prepare: () => ({ idempotencyKey: newBotId("idem") }),
      execute: ({ idempotencyKey }) => {
        const currentVersion =
          kind === "bot"
            ? snapshot()?.bots.find((item) => item.bot_id === id)?.event_version
            : snapshot()?.runs.find((item) => item.spec.run_id === id)?.event_version;
        if (currentVersion === undefined) throw new Error("Recovery target is unavailable");
        return mode === "repair"
          ? botRecoveryRepair(kind, id, currentVersion, idempotencyKey)
          : botRecoveryClear(kind, id, currentVersion, idempotencyKey);
      },
      applied: () => {
        const current = snapshot();
        if (!current) return false;
        const registryClosed = !current.registry.some(
          (record) => record.aggregate.kind === kind && record.aggregate.id === id,
        );
        return mode === "repair"
          ? registryClosed &&
              current.bots.some((bot) => bot.bot_id === id && bot.lifecycle === "paused")
          : registryClosed;
      },
      okText:
        mode === "repair"
          ? "修复证据已确认，Bot 转为 paused"
          : "未完成工作已明确放弃，Recovery record 已关闭",
      errPrefix: "Recovery 操作失败",
    });
  };
  const count = () => {
    const value = snapshot();
    return value
      ? value.registry.length +
          value.bots.length +
          value.runs.length +
          value.conversations.length +
          value.routines.length
      : 0;
  };

  return (
    <div class="space-y-4">
      <Panel
        title="Recovery Center"
        detail="UNKNOWN 不是失败重试信号。只有核对副作用证据后才能 repair，或明确放弃未完成工作后 clear。"
      >
        <Show when={loadErr()}>
          <p class="text-xs text-[var(--err)] mb-2">{loadErr()}</p>
        </Show>
        <div class="text-sm">
          <span class={count() ? "text-[var(--warn)]" : "text-[var(--ok)]"}>
            {count() ? `${count()} 个待处理状态` : "PASS，没有待处理 Recovery"}
          </span>
        </div>
      </Panel>
      <Panel
        title="Durable Recovery Registry"
        detail="Registry 只保存发现和处理入口，aggregate event stream 始终是真源。"
      >
        <div class="space-y-3">
          <For
            each={snapshot()?.registry ?? []}
            fallback={<p class="text-xs text-[var(--text-faint)]">暂无 open Recovery record。</p>}
          >
            {(record) => (
              <div class="rounded border border-[var(--warn)]/50 p-3 text-xs">
                <div class="flex gap-2">
                  <span class="text-[var(--warn)]">UNKNOWN</span>
                  <span>{record.aggregate.kind}</span>
                  <span class="font-mono">{shortId(record.aggregate.id)}</span>
                </div>
                <p class="selectable my-2">{record.reason}</p>
                <For each={record.evidence}>
                  {(evidence) => (
                    <div class="font-mono text-2xs text-[var(--text-faint)] break-all">
                      {evidence}
                    </div>
                  )}
                </For>
                <Show
                  when={
                    record.aggregate.kind === "bot" &&
                    snapshot()?.bots.some((bot) => bot.bot_id === record.aggregate.id)
                  }
                >
                  <div class="flex gap-2 mt-3">
                    <button
                      class={actionClass}
                      disabled={acting()}
                      onClick={() => void resolve("bot", record.aggregate.id, "repair")}
                    >
                      证据已修复
                    </button>
                    <button
                      class={actionClass}
                      disabled={acting()}
                      onClick={() => void resolve("bot", record.aggregate.id, "clear")}
                    >
                      清除未完成工作
                    </button>
                  </div>
                </Show>
                <Show
                  when={
                    record.aggregate.kind === "bot_run" &&
                    snapshot()?.runs.some((run) => run.spec.run_id === record.aggregate.id)
                  }
                >
                  <div class="mt-3">
                    <button
                      class={actionClass}
                      disabled={acting()}
                      onClick={() => void resolve("bot_run", record.aggregate.id, "clear")}
                    >
                      确认放弃，不重试 UNKNOWN effect
                    </button>
                  </div>
                </Show>
                <Show when={!["bot", "bot_run"].includes(record.aggregate.kind)}>
                  <p class="text-2xs text-[var(--text-faint)] mt-2">
                    该类型必须回到来源 aggregate 修复，不能在此强制解锁。
                  </p>
                </Show>
              </div>
            )}
          </For>
        </div>
      </Panel>
      <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
        <BlockedList
          title="Blocked Runs"
          items={(snapshot()?.runs ?? []).map((run) => ({
            id: run.spec.run_id,
            reason: run.error_message || run.error_code || "blocked",
            status: run.status,
          }))}
        />
        <BlockedList
          title="Blocked Conversations"
          items={(snapshot()?.conversations ?? []).map((conversation) => ({
            id: conversation.conversation_id,
            reason: "conversation recovery required",
            status: conversation.lifecycle,
          }))}
        />
        <BlockedList
          title="Blocked Routines"
          items={(snapshot()?.routines ?? []).map((routine) => ({
            id: routine.routine_id,
            reason: routine.blocked_reason || "routine recovery required",
            status: routine.lifecycle,
          }))}
        />
        <BlockedList
          title="Blocked Bots"
          items={(snapshot()?.bots ?? []).map((bot) => ({
            id: bot.bot_id,
            reason: bot.blocked_reason || "bot recovery required",
            status: bot.lifecycle,
          }))}
        />
      </div>
    </div>
  );
}

function BlockedList(props: {
  title: string;
  items: Array<{ id: string; reason: string; status: string }>;
}) {
  return (
    <Panel title={props.title}>
      <div class="space-y-2">
        <For each={props.items} fallback={<p class="text-xs text-[var(--text-faint)]">无</p>}>
          {(item) => (
            <div class="rounded border border-[var(--border)] p-2 text-xs">
              <div class="flex gap-2">
                <span class={statusClass(item.status)}>{item.status}</span>
                <span class="font-mono">{shortId(item.id)}</span>
              </div>
              <p class="text-[var(--text-dim)] mt-1">{item.reason}</p>
            </div>
          )}
        </For>
      </div>
    </Panel>
  );
}
