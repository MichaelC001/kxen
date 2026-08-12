import { For, Show, createEffect, createSignal } from "solid-js";
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
import { flashErr, flashOk } from "../../lib/flash";
import { formatError } from "../../lib/error-text";
import { Panel, shortId, statusClass, type RefreshProps } from "./shared";
import BotLibraryDetail from "./BotLibraryDetail";

export default function BotLibrary(props: RefreshProps) {
  const [bots, setBots] = createSignal<BotSummary[]>([]);
  const [selectedId, setSelectedId] = createSignal("");
  const [detail, setDetail] = createSignal<BotState | null>(null);
  const [memory, setMemory] = createSignal<BotMemoryState | null>(null);
  const [loadErr, setLoadErr] = createSignal("");
  const [acting, setActing] = createSignal(false);
  const [prompt, setPrompt] = createSignal("");
  const [memoryText, setMemoryText] = createSignal("");
  const [memoryKind, setMemoryKind] = createSignal("fact");
  const [editingMemory, setEditingMemory] = createSignal("");
  let loadSeq = 0;

  const reload = async () => {
    const seq = ++loadSeq;
    try {
      const list = await botList();
      if (seq !== loadSeq) return;
      setBots(list);
      const wanted = selectedId() || list[0]?.bot_id || "";
      if (!wanted) {
        setDetail(null);
        setMemory(null);
        setLoadErr("");
        return;
      }
      setSelectedId(wanted);
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
    setEditingMemory("");
    setMemoryText("");
    void reload();
  };
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
  const lifecycle = (kind: "pause" | "resume" | "archive" | "trash" | "restore") => {
    const state = detail();
    if (!state) return;
    const key = newBotId("idem");
    const jobs = {
      pause: () => botPause(state.bot_id, state.event_version, key),
      resume: () => botResume(state.bot_id, state.event_version, key),
      archive: () => botArchive(state.bot_id, state.event_version, key),
      trash: () => botTrash(state.bot_id, state.event_version, key),
      restore: () => botRestore(state.bot_id, state.event_version, key),
    };
    void act(jobs[kind], `${kind} 已提交`);
  };
  const run = () => {
    const state = detail();
    const text = prompt().trim();
    if (!state || !text) return;
    void act(
      () => botRunStart(newBotId("brun"), state.bot_id, [{ kind: "text", text }], newBotId("idem")),
      "BotRun 已排队",
    );
  };
  const duplicate = () => {
    const state = detail();
    if (!state?.current_revision_id) return;
    const definition = currentDefinition(state);
    const id = newBotId("bot");
    void act(
      () =>
        botDuplicate(
          state.bot_id,
          id,
          `${definition?.display_name ?? "Bot"} Copy`,
          newBotId("idem"),
        ),
      "Bot 已复制",
    );
  };
  const saveMemory = () => {
    const state = detail();
    const memoryState = memory();
    const content = memoryText().trim();
    if (!state || !memoryState || !content) return;
    const item = editingMemory() ? memoryState.items[editingMemory()] : undefined;
    const job = item
      ? () =>
          botMemoryRevise(
            state.bot_id,
            item.item_id,
            item.version,
            content,
            memoryState.event_version,
            newBotId("idem"),
          )
      : () =>
          botMemoryCreate(
            state.bot_id,
            newBotId("memory"),
            memoryKind(),
            content,
            memoryState.event_version,
            newBotId("idem"),
          );
    void act(
      async () => {
        await job();
        setEditingMemory("");
        setMemoryText("");
      },
      item ? "Memory 已更新" : "Memory 已创建",
    );
  };
  const removeMemory = (itemId: string, version: number) => {
    const state = detail();
    const memoryState = memory();
    if (!state || !memoryState) return;
    void act(
      () =>
        botMemoryRemove(state.bot_id, itemId, version, memoryState.event_version, newBotId("idem")),
      "Memory 已删除",
    );
  };

  return (
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-4">
      <Panel title="Bot Library" detail="草稿、发布版本和生命周期在这里统一管理。">
        <Show when={loadErr()}>
          <p class="text-xs text-[var(--err)] mb-2">{loadErr()}</p>
        </Show>
        <div class="space-y-2">
          <For
            each={bots()}
            fallback={
              <p class="text-xs text-[var(--text-faint)]">还没有 Bot，请从 Bot Build 创建。</p>
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

function currentDefinition(state: BotState) {
  if (state.draft) return state.draft.definition;
  return Object.values(state.revisions).sort(
    (left, right) => right.revision_number - left.revision_number,
  )[0]?.definition;
}
