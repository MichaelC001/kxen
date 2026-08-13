import { For, Show, createEffect, createMemo, createSignal } from "solid-js";
import {
  botArchive,
  botDuplicate,
  botGet,
  botList,
  botMemoryCreate,
  botMemoryList,
  botMemoryRemove,
  botMemoryRevise,
  botPause,
  botRestore,
  botResume,
  botRunStart,
  botTrash,
  newBotId,
  type BotMemoryState,
  type BotState,
  type BotSummary,
} from "../../lib/bots";
import { createReconciledMutation } from "../../lib/async-guard";
import { flashErr } from "../../lib/flash";
import { formatError } from "../../lib/error-text";
import { editableBotDefinition, encodeBotInput, publishedBotDefinition } from "./bot-definition";
import {
  fieldClass,
  Panel,
  shortId,
  statusClass,
  type BotBuilderTarget,
  type RefreshProps,
} from "./shared";
import BotLibraryDetail from "./BotLibraryDetail";

export default function BotLibrary(
  props: RefreshProps & { onBuild: (target: BotBuilderTarget) => void },
) {
  const [bots, setBots] = createSignal<BotSummary[]>([]);
  const [selectedId, setSelectedId] = createSignal("");
  const [detail, setDetail] = createSignal<BotState | null>(null);
  const [memory, setMemory] = createSignal<BotMemoryState | null>(null);
  const [loadErr, setLoadErr] = createSignal("");
  const [prompt, setPrompt] = createSignal("");
  const [memoryText, setMemoryText] = createSignal("");
  const [memoryKind, setMemoryKind] = createSignal("fact");
  const [editingMemory, setEditingMemory] = createSignal("");
  const [query, setQuery] = createSignal("");
  const [lifecycleFilter, setLifecycleFilter] = createSignal("");
  let loadSeq = 0;

  const visibleBots = createMemo(() => {
    const needle = query().trim().toLocaleLowerCase();
    const lifecycle = lifecycleFilter();
    return bots().filter(
      (bot) =>
        (!lifecycle || bot.lifecycle === lifecycle) &&
        (!needle ||
          bot.display_name.toLocaleLowerCase().includes(needle) ||
          bot.bot_id.toLocaleLowerCase().includes(needle)),
    );
  });

  const reload = async () => {
    const seq = ++loadSeq;
    try {
      const list = await botList(true);
      if (seq !== loadSeq) return;
      setBots(list);
      const selected = selectedId();
      const wanted = list.some((bot) => bot.bot_id === selected) ? selected : list[0]?.bot_id || "";
      if (!wanted) {
        setDetail(null);
        setMemory(null);
        setLoadErr("");
        return;
      }
      setSelectedId(wanted);
      if (detail()?.bot_id !== wanted) {
        setDetail(null);
        setMemory(null);
      }
      const [state, items] = await Promise.all([botGet(wanted), botMemoryList(wanted)]);
      if (seq !== loadSeq) return;
      setDetail(state);
      setMemory(items);
      setLoadErr("");
    } catch (error) {
      if (seq === loadSeq) setLoadErr(formatError(error));
    }
  };
  createEffect(() => {
    void props.epoch;
    void reload();
  });

  const select = (id: string) => {
    setSelectedId(id);
    setDetail(null);
    setMemory(null);
    setEditingMemory("");
    setMemoryText("");
    void reload();
  };
  const mutation = createReconciledMutation({ refresh: reload, onChanged: props.onChanged });
  const acting = mutation.pending;
  const lifecycle = (kind: "pause" | "resume" | "archive" | "trash" | "restore") => {
    const state = detail();
    if (!state) return;
    const botId = state.bot_id;
    const expectedLifecycle = {
      pause: "paused",
      resume: "active",
      archive: "archived",
      trash: "trashed",
      restore: "paused",
    }[kind];
    void mutation.run({
      key: `bot:${botId}:lifecycle:${kind}`,
      prepare: () => ({ idempotencyKey: newBotId("idem") }),
      execute: ({ idempotencyKey }) => {
        const current = detail();
        if (!current || current.bot_id !== botId) throw new Error("selected Bot changed");
        const jobs = {
          pause: () => botPause(botId, current.event_version, idempotencyKey),
          resume: () => botResume(botId, current.event_version, idempotencyKey),
          archive: () => botArchive(botId, current.event_version, idempotencyKey),
          trash: () => botTrash(botId, current.event_version, idempotencyKey),
          restore: () => botRestore(botId, current.event_version, idempotencyKey),
        };
        return jobs[kind]();
      },
      applied: () => detail()?.bot_id === botId && detail()?.lifecycle === expectedLifecycle,
      okText: `${kind} 已提交`,
    });
  };
  const run = () => {
    const state = detail();
    const text = prompt().trim();
    if (!state || !text) return;
    let input;
    try {
      input = encodeBotInput(text, publishedBotDefinition(state));
    } catch (error) {
      flashErr(`输入契约校验失败：${formatError(error)}`);
      return;
    }
    const botId = state.bot_id;
    void mutation.run({
      key: `bot:${botId}:run:${text}`,
      prepare: () => ({ runId: newBotId("brun"), idempotencyKey: newBotId("idem") }),
      execute: ({ runId, idempotencyKey }) => botRunStart(runId, botId, input, idempotencyKey),
      onApplied: () => setPrompt(""),
      okText: "BotRun 已排队",
    });
  };
  const duplicate = () => {
    const state = detail();
    if (!state?.current_revision_id) return;
    const definition = publishedBotDefinition(state);
    const sourceBotId = state.bot_id;
    const displayName = `${definition?.display_name ?? "Bot"} Copy`;
    void mutation.run({
      key: `bot:${sourceBotId}:duplicate:${state.current_revision_id}:${displayName}`,
      prepare: () => ({ botId: newBotId("bot"), idempotencyKey: newBotId("idem") }),
      execute: ({ botId, idempotencyKey }) =>
        botDuplicate(sourceBotId, botId, displayName, idempotencyKey),
      applied: ({ botId }) => bots().some((bot) => bot.bot_id === botId),
      okText: "Bot 已复制",
    });
  };
  const saveMemory = () => {
    const state = detail();
    const memoryState = memory();
    const content = memoryText().trim();
    if (!state || !memoryState || !content) return;
    const item = editingMemory() ? memoryState.items[editingMemory()] : undefined;
    const botId = state.bot_id;
    const kind = memoryKind();
    const operationKey = item
      ? `bot:${botId}:memory:revise:${item.item_id}:${content}`
      : `bot:${botId}:memory:create:${kind}:${content}`;
    void mutation.run({
      key: operationKey,
      prepare: () => ({
        itemId: item?.item_id ?? newBotId("memory"),
        idempotencyKey: newBotId("idem"),
      }),
      execute: ({ itemId, idempotencyKey }) => {
        const current = memory();
        if (!current) throw new Error("Bot Memory state is unavailable");
        const currentItem = current.items[itemId];
        return item
          ? botMemoryRevise(
              botId,
              itemId,
              currentItem?.version ?? item.version,
              content,
              current.event_version,
              idempotencyKey,
            )
          : botMemoryCreate(botId, itemId, kind, content, current.event_version, idempotencyKey);
      },
      applied: ({ itemId }) => memory()?.items[itemId]?.content === content,
      onApplied: () => {
        setEditingMemory("");
        setMemoryText("");
      },
      okText: item ? "Memory 已更新" : "Memory 已创建",
    });
  };
  const removeMemory = (itemId: string, version: number) => {
    const state = detail();
    const memoryState = memory();
    if (!state || !memoryState) return;
    const botId = state.bot_id;
    void mutation.run({
      key: `bot:${botId}:memory:remove:${itemId}:${version}`,
      prepare: () => ({ idempotencyKey: newBotId("idem") }),
      execute: ({ idempotencyKey }) => {
        const current = memory();
        if (!current) throw new Error("Bot Memory state is unavailable");
        const currentItem = current.items[itemId];
        return botMemoryRemove(
          botId,
          itemId,
          currentItem?.version ?? version,
          current.event_version,
          idempotencyKey,
        );
      },
      applied: () => {
        const current = memory();
        return Boolean(current && !current.items[itemId]);
      },
      okText: "Memory 已删除",
    });
  };

  return (
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-4">
      <Panel title="Bot Library" detail="草稿、发布版本和生命周期在这里统一管理。">
        <Show when={loadErr()}>
          <p class="text-xs text-[var(--err)] mb-2">{loadErr()}</p>
        </Show>
        <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-1 gap-2 mb-3">
          <input
            class={fieldClass}
            type="search"
            aria-label="搜索 Bot"
            value={query()}
            onInput={(event) => setQuery(event.currentTarget.value)}
            placeholder="按名称或 ID 搜索"
          />
          <select
            class="form-select"
            aria-label="按生命周期筛选 Bot"
            value={lifecycleFilter()}
            onChange={(event) => setLifecycleFilter(event.currentTarget.value)}
          >
            <option value="">全部状态</option>
            <option value="draft">draft</option>
            <option value="active">active</option>
            <option value="paused">paused</option>
            <option value="archived">archived</option>
            <option value="trashed">trashed</option>
            <option value="blocked">blocked</option>
          </select>
        </div>
        <div class="space-y-2">
          <For
            each={visibleBots()}
            fallback={
              <p class="text-xs text-[var(--text-faint)]">
                {bots().length ? "没有匹配的 Bot。" : "还没有 Bot，请从“创建 Bot”开始。"}
              </p>
            }
          >
            {(bot) => (
              <button
                class="pressable w-full text-left rounded border p-2.5"
                classList={{
                  "border-[var(--accent)] bg-[var(--bg-overlay)]": selectedId() === bot.bot_id,
                  "border-[var(--border)]": selectedId() !== bot.bot_id,
                }}
                onClick={() => select(bot.bot_id)}
              >
                <div class="flex items-center gap-2">
                  <span class="text-sm truncate">{bot.display_name || bot.bot_id}</span>
                  <span class={`ml-auto text-2xs ${statusClass(bot.lifecycle)}`}>
                    {bot.lifecycle}
                  </span>
                </div>
                <div class="text-2xs text-[var(--text-faint)] font-mono mt-1">
                  {shortId(bot.bot_id)}
                </div>
              </button>
            )}
          </For>
        </div>
      </Panel>

      <div class="lg:col-span-2 space-y-4">
        <Show
          when={detail()}
          fallback={
            <Panel title="Bot 详情">
              <p class="text-xs text-[var(--text-faint)]">选择一个 Bot 查看详情。</p>
            </Panel>
          }
        >
          {(state) => (
            <BotLibraryDetail
              state={state()}
              memory={memory()}
              acting={acting()}
              prompt={prompt()}
              memoryText={memoryText()}
              memoryKind={memoryKind()}
              editingMemory={editingMemory()}
              lifecycle={lifecycle}
              duplicate={duplicate}
              run={run}
              saveMemory={saveMemory}
              removeMemory={removeMemory}
              build={() =>
                props.onBuild({
                  bot_id: state().bot_id,
                  display_name: editableBotDefinition(state())?.display_name ?? state().bot_id,
                })
              }
              setPrompt={setPrompt}
              setMemoryText={setMemoryText}
              setMemoryKind={setMemoryKind}
              setEditingMemory={setEditingMemory}
            />
          )}
        </Show>
      </div>
    </div>
  );
}
