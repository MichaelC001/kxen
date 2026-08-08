// 看板页（/kanban/:board?workspace=<path>）：列横排 + 卡片详情 + 人工动作 + 自主授权。
// 数据流：kanban.snapshot 全量重拉（重连恢复口径）+ kanban:<board> topic 250ms 去抖 + resync + 8s 轮询。
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { A, useParams, useSearchParams } from "@solidjs/router";
import { ArrowLeft, Plus, ShieldCheck } from "lucide-solid";
import {
  kanbanCardComment,
  kanbanCardCreate,
  kanbanCardMove,
  kanbanPolicySet,
  kanbanRunStart,
  kanbanSnapshot,
  onTopic,
  type KanbanCard,
  type KanbanPolicySpec,
  type KanbanSnapshot,
} from "../lib/chat";
import { client } from "../lib/client";
import { flashErr } from "../lib/flash";
import { formatError } from "../lib/error-text";
import { KANBAN_TONE_CLASS, kanbanStatusMeta } from "../lib/board";
import { createSeqGuard } from "../lib/async-guard";
import { isTauri } from "../lib/runtime";
import { onDragStart } from "../lib/drag";
import KanbanCardDetail from "../components/KanbanCardDetail";
import KanbanPolicy from "../components/KanbanPolicy";

export default function Kanban() {
  const params = useParams<{ board: string }>();
  const [search] = useSearchParams<{ workspace?: string }>();
  const workspace = () => search.workspace ?? "";
  const board = () => params.board;

  const [snapshot, setSnapshot] = createSignal<KanbanSnapshot | null>(null);
  const [loadErr, setLoadErr] = createSignal("");
  const [loaded, setLoaded] = createSignal(false);
  const [selected, setSelected] = createSignal<string | null>(null);
  const [showNewCard, setShowNewCard] = createSignal(false);
  const [showPolicy, setShowPolicy] = createSignal(false);
  const [acting, setActing] = createSignal(false);
  const [newTitle, setNewTitle] = createSignal("");
  const [newBody, setNewBody] = createSignal("");
  const reloadGuard = createSeqGuard();
  let unlisten: (() => void) | undefined;
  let offResync: (() => void) | undefined;
  let timer: ReturnType<typeof setInterval> | undefined;
  let eventTimer: ReturnType<typeof setTimeout> | undefined;

  const reload = async () => {
    const request = reloadGuard.next();
    // 失败保留旧值但记错误态：首载失败必须与真空（板还没有卡）区分
    const snap = await kanbanSnapshot(workspace(), board()).catch((e: unknown) => {
      if (reloadGuard.isCurrent(request)) setLoadErr(formatError(e));
      return null;
    });
    if (!reloadGuard.isCurrent(request)) return;
    if (snap) {
      setSnapshot(snap);
      setLoadErr("");
    }
    setLoaded(true);
  };

  // KanbanUpdate 只承担失效通知：连发帧（runner 批量落地）250ms 去抖合并成一次全量重拉
  const bump = () => {
    if (eventTimer) clearTimeout(eventTimer);
    eventTimer = setTimeout(() => {
      eventTimer = undefined;
      void reload();
    }, 250);
  };

  onMount(() => {
    void reload();
    unlisten = onTopic([`kanban:${board()}`], bump);
    // topic 丢帧后不自愈：resync 信号按真源重拉（同 Workspaces 模式）
    offResync = client.onResync(() => void reload());
    timer = setInterval(() => void reload(), 8000);
  });
  onCleanup(() => {
    unlisten?.();
    offResync?.();
    if (timer) clearInterval(timer);
    if (eventTimer) clearTimeout(eventTimer);
  });

  /** 人工动作统一入口：进行中禁用连点，失败 flash，成功后按真源重拉。 */
  const act = async (task: () => Promise<unknown>, errPrefix: string) => {
    if (acting()) return;
    setActing(true);
    try {
      await task();
      await reload();
    } catch (e) {
      flashErr(`${errPrefix}：${formatError(e)}`);
    } finally {
      setActing(false);
    }
  };

  const selectedCard = (): KanbanCard | null => {
    const id = selected();
    return (id && snapshot()?.cards[id]) || null;
  };
  const columnOf = (card: KanbanCard) => snapshot()?.columns.find((c) => c.id === card.column_id);
  const cardsIn = (columnId: string) =>
    Object.values(snapshot()?.cards ?? {}).filter((c) => c.column_id === columnId);

  const createCard = () =>
    act(async () => {
      await kanbanCardCreate(workspace(), board(), newTitle().trim(), newBody().trim());
      setNewTitle("");
      setNewBody("");
      setShowNewCard(false);
    }, "新建卡片失败");

  const savePolicy = (policy: KanbanPolicySpec) =>
    act(async () => {
      await kanbanPolicySet(workspace(), board(), policy);
      setShowPolicy(false);
    }, "保存授权失败");

  return (
    <div class="h-full flex-1 overflow-auto">
      {/* 拖拽占位条只在 Tauri 无边框窗口下需要 */}
      <Show when={isTauri()}>
        <div class="h-8" data-tauri-drag-region onMouseDown={onDragStart} />
      </Show>
      <div class="px-8 py-6 pt-2">
        <A
          href="/workspaces"
          class="inline-flex items-center gap-1.5 text-xs text-[var(--text-dim)] hover:text-[var(--text)] mb-4"
        >
          <ArrowLeft size={13} />
          返回工作看板
        </A>
        <div class="flex items-center gap-2 mb-4">
          <h1 class="text-lg font-medium text-[var(--text)] truncate">
            {snapshot()?.title ?? board()}
          </h1>
          <div class="ml-auto flex items-center gap-2 shrink-0">
            <button
              class="pressable px-2.5 py-1 rounded border border-[var(--border)] text-xs flex items-center gap-1"
              onClick={() => setShowNewCard(!showNewCard())}
            >
              <Plus size={11} />
              新建卡片
            </button>
            <button
              class="pressable px-2.5 py-1 rounded border border-[var(--border)] text-xs flex items-center gap-1"
              classList={{ "text-[var(--ok)]": !!snapshot()?.policy }}
              onClick={() => setShowPolicy(!showPolicy())}
            >
              <ShieldCheck size={11} />
              授权
            </button>
          </div>
        </div>

        <Show when={showNewCard()}>
          <div class="mb-3 max-w-lg rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] p-3 space-y-2">
            <input
              class="w-full px-2 py-1.5 rounded border border-[var(--border)] bg-transparent text-xs"
              placeholder="卡片标题"
              aria-label="卡片标题"
              value={newTitle()}
              onInput={(e) => setNewTitle(e.currentTarget.value)}
            />
            <textarea
              class="w-full px-2 py-1.5 rounded border border-[var(--border)] bg-transparent text-xs resize-none"
              rows={3}
              placeholder="正文（可选）：交给列 Agent 的任务描述"
              aria-label="卡片正文"
              value={newBody()}
              onInput={(e) => setNewBody(e.currentTarget.value)}
            />
            <div class="flex items-center gap-2">
              <button
                class="pressable px-3 py-1.5 rounded border border-[var(--border)] text-xs disabled:opacity-40"
                disabled={acting() || !newTitle().trim()}
                onClick={() => void createCard()}
              >
                创建
              </button>
              <button
                class="pressable px-3 py-1.5 rounded border border-[var(--border)] text-xs text-[var(--text-dim)]"
                onClick={() => setShowNewCard(false)}
              >
                取消
              </button>
            </div>
          </div>
        </Show>

        <Show when={showPolicy()}>
          <div class="mb-3 max-w-lg">
            <KanbanPolicy
              policy={snapshot()?.policy}
              acting={acting()}
              onSave={(p) => void savePolicy(p)}
              onClose={() => setShowPolicy(false)}
            />
          </div>
        </Show>

        <Show when={!loaded()}>
          <div class="text-xs text-[var(--text-faint)]">加载中…</div>
        </Show>
        <Show when={loaded() && loadErr()}>
          <div class="mb-3 max-w-md rounded-lg border border-[var(--err)]/50 bg-[var(--err)]/5 px-3 py-2 flex items-center gap-3">
            <span class="text-xs text-[var(--err)]">
              {snapshot() ? "刷新看板失败，正在显示上次结果" : "加载看板失败"}：{loadErr()}
            </span>
            <button
              class="pressable px-2 py-0.5 rounded border border-[var(--border)] text-xs text-[var(--text-dim)]"
              onClick={() => void reload()}
            >
              重试
            </button>
          </div>
        </Show>

        <Show when={snapshot()}>
          {(snap) => (
            <div class="flex items-start gap-3 pb-4">
              <div class="flex items-start gap-3 overflow-x-auto">
                <For each={snap().columns}>
                  {(column) => {
                    const cards = () => cardsIn(column.id);
                    return (
                      <div class="w-64 shrink-0 rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] flex flex-col">
                        <div class="flex items-center gap-1.5 px-3 py-2 border-b border-[var(--border)]">
                          <span class="text-xs font-medium text-[var(--text)] truncate">
                            {column.title}
                          </span>
                          <span class="ml-auto text-2xs tabular-nums text-[var(--text-faint)] shrink-0">
                            {column.wip_limit != null
                              ? `${cards().length}/${column.wip_limit}`
                              : cards().length}
                          </span>
                        </div>
                        <div class="p-2 space-y-1.5">
                          <For
                            each={cards()}
                            fallback={
                              <div class="px-2 py-3 text-2xs text-[var(--text-faint)]">空</div>
                            }
                          >
                            {(card) => {
                              const meta = () => kanbanStatusMeta(card.status);
                              return (
                                <button
                                  class="pressable w-full text-left px-2 py-1.5 rounded border hover:border-[var(--text-dim)] transition-colors"
                                  classList={{
                                    "border-[var(--text-dim)]": selected() === card.id,
                                    "border-[var(--border)]": selected() !== card.id,
                                  }}
                                  onClick={() =>
                                    setSelected(selected() === card.id ? null : card.id)
                                  }
                                >
                                  <div class="text-xs text-[var(--text)] break-words">
                                    {card.title}
                                  </div>
                                  <div class="mt-0.5 flex items-center gap-1 text-2xs">
                                    <Show when={card.status === "running"}>
                                      <span class="w-1.5 h-1.5 rounded-full bg-[var(--ok)] animate-pulse" />
                                    </Show>
                                    <span class={KANBAN_TONE_CLASS[meta().tone]}>
                                      {meta().label}
                                    </span>
                                    <Show when={card.comments.length > 0}>
                                      <span class="text-[var(--text-faint)]">
                                        {card.comments.length} 评论
                                      </span>
                                    </Show>
                                  </div>
                                </button>
                              );
                            }}
                          </For>
                        </div>
                      </div>
                    );
                  }}
                </For>
              </div>
              <Show when={selectedCard()}>
                {(card) => (
                  <KanbanCardDetail
                    card={card()}
                    column={columnOf(card())}
                    acting={acting()}
                    onClose={() => setSelected(null)}
                    onMove={(outcome) =>
                      void act(
                        () => kanbanCardMove(workspace(), board(), card().id, outcome),
                        outcome === "success" ? "通过失败" : "打回失败",
                      )
                    }
                    onRetry={() =>
                      void act(() => kanbanRunStart(workspace(), board(), card().id), "重试失败")
                    }
                    onComment={(body) =>
                      void act(
                        () => kanbanCardComment(workspace(), board(), card().id, body),
                        "评论失败",
                      )
                    }
                  />
                )}
              </Show>
            </div>
          )}
        </Show>
      </div>
    </div>
  );
}
