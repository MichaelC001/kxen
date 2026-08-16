// Trajectory 检视视图：会话事件流的事件级 read model（与 Chat 双视图并存）。
// 工具栏（Duration/Collapse turns/Collapse calls/搜索）+ Overview 拖选聚焦 + 虚拟化记录表
// （TrajectoryList：尾部优先分页、触顶加载更早、跟随尾部）+ Inspector 侧栏。
import { createEffect, createMemo, createSignal, Show, type Accessor } from "solid-js";
import { sessionMessages, type StoredMessage } from "../lib/chat";
import type { InspectTarget } from "../lib/session-view";
import {
  filterTrajectory,
  groupTrajectoryTurns,
  recordsInRange,
  toTrajectoryRecords,
  trajectoryTailWindow,
  overviewBars,
  type TrajectoryTurn,
} from "../lib/trajectory";
import TrajectoryInspector from "./TrajectoryInspector";
import TrajectoryList from "./TrajectoryList";
import TrajectoryOverview, { type TimeRange } from "./TrajectoryOverview";

const PAGE = 100;

export default function TrajectoryView(props: {
  sessionId: Accessor<string>;
  active: Accessor<boolean>;
  streaming: Accessor<boolean>;
  focus: Accessor<InspectTarget | null>;
  onFocusConsumed: () => void;
}) {
  const [messages, setMessages] = createSignal<StoredMessage[]>([]);
  const [loadErr, setLoadErr] = createSignal("");
  const [showDuration, setShowDuration] = createSignal(true);
  const [collapseTurns, setCollapseTurns] = createSignal(false);
  const [collapseCalls, setCollapseCalls] = createSignal(false);
  const [query, setQuery] = createSignal("");
  const [committedQuery, setCommittedQuery] = createSignal("");
  const [selection, setSelection] = createSignal<TimeRange | undefined>();
  const [selectedIndex, setSelectedIndex] = createSignal<number | undefined>();
  const [limit, setLimit] = createSignal(PAGE);
  const [expandedTurns, setExpandedTurns] = createSignal<Set<number>>(new Set());
  const [focusIndex, setFocusIndex] = createSignal<number | undefined>();
  let queryTimer: ReturnType<typeof setTimeout> | undefined;

  const load = (id: string) => {
    if (!id) return;
    setLoadErr("");
    void sessionMessages(id)
      .then((loaded) => {
        if (props.sessionId() === id) setMessages(loaded);
      })
      .catch((error: unknown) => {
        if (props.sessionId() === id)
          setLoadErr(error instanceof Error ? error.message : String(error));
      });
  };

  // 激活/换会话/流式结束（true->false）时拉取；检视数据以落盘快照为权威
  let wasStreaming = false;
  let loadedSid = "";
  createEffect(() => {
    const streaming = props.streaming();
    const id = props.sessionId();
    if (props.active() && id && (loadedSid !== id || wasStreaming)) {
      if (loadedSid !== id) {
        // 换会话：分页/选区/选中态全部复位，旧会话记录不留
        setLimit(PAGE);
        setSelection(undefined);
        setSelectedIndex(undefined);
        setFocusIndex(undefined);
        setMessages([]);
      }
      loadedSid = id;
      load(id);
    }
    wasStreaming = streaming;
  });

  const records = createMemo(() => toTrajectoryRecords(messages()));
  const searched = createMemo(() => filterTrajectory(records(), committedQuery()));
  const ranged = createMemo(() => {
    const range = selection();
    if (!range) return searched();
    const hit = recordsInRange(searched(), overviewBars(searched()), range.start, range.end);
    return searched().filter((r) => hit.has(r.index));
  });
  const turns = createMemo(() => groupTrajectoryTurns(ranged()));
  // 尾部优先分页：先取尾部记录窗，再按 turn 重组保证边界完整（半个 turn 不显示）
  const visibleTurns = createMemo(() => {
    const { window } = trajectoryTailWindow(ranged(), limit());
    if (!window.length) return [] as TrajectoryTurn[];
    const first = window[0]!.index;
    return turns()
      .map((turn) => ({ ...turn, records: turn.records.filter((r) => r.index >= first) }))
      .filter((turn) => turn.records.length > 0);
  });
  const hasEarlier = createMemo(() => trajectoryTailWindow(ranged(), limit()).hasEarlier);
  const earlierLabel = () =>
    `加载更早 ${Math.min(PAGE, ranged().length - limit())} 条（共 ${ranged().length} 条）`;

  // Inspect 联动：清过滤定位记录、扩窗包含它、选中并交给列表滚到
  createEffect(() => {
    const target = props.focus();
    if (!target || !props.active()) return;
    const all = records();
    const position = all.findIndex(
      (r) => r.messageId === target.messageId && r.partIndex === target.partIndex,
    );
    if (position < 0) return;
    const record = all[position]!;
    setCommittedQuery("");
    setQuery("");
    setSelection(undefined);
    setLimit((current) => Math.max(current, all.length - position));
    setSelectedIndex(record.index);
    setFocusIndex(record.index);
    props.onFocusConsumed();
  });

  const onSearchInput = (value: string) => {
    setQuery(value);
    if (queryTimer) clearTimeout(queryTimer);
    queryTimer = setTimeout(() => setCommittedQuery(value), 200);
  };

  const toggleTurn = (turn: TrajectoryTurn) => {
    setExpandedTurns((previous) => {
      const next = new Set(previous);
      if (next.has(turn.startIndex)) next.delete(turn.startIndex);
      else next.add(turn.startIndex);
      return next;
    });
  };
  // Collapse turns 关闭时清空局部展开状态
  createEffect(() => {
    if (!collapseTurns()) setExpandedTurns(new Set<number>());
  });

  const selectedRecord = createMemo(() => {
    const index = selectedIndex();
    return index === undefined ? undefined : records().find((r) => r.index === index);
  });

  return (
    <div data-testid="trajectory-view" class="flex-1 min-h-0 flex flex-col">
      <div class="flex items-center gap-2 px-4 py-1.5 border-b border-[var(--border)]">
        <button
          class="pressable px-2 py-0.5 rounded border border-[var(--border)] text-2xs"
          classList={{
            "bg-[var(--bg-overlay)] text-[var(--text)]": showDuration(),
            "text-[var(--text-dim)]": !showDuration(),
          }}
          onClick={() => setShowDuration(!showDuration())}
        >
          Duration
        </button>
        <button
          class="pressable px-2 py-0.5 rounded border border-[var(--border)] text-2xs"
          classList={{
            "bg-[var(--bg-overlay)] text-[var(--text)]": collapseTurns(),
            "text-[var(--text-dim)]": !collapseTurns(),
          }}
          onClick={() => setCollapseTurns(!collapseTurns())}
        >
          Collapse turns
        </button>
        <button
          class="pressable px-2 py-0.5 rounded border border-[var(--border)] text-2xs"
          classList={{
            "bg-[var(--bg-overlay)] text-[var(--text)]": collapseCalls(),
            "text-[var(--text-dim)]": !collapseCalls(),
          }}
          onClick={() => setCollapseCalls(!collapseCalls())}
        >
          Collapse calls
        </button>
        <input
          data-testid="trajectory-search"
          class="flex-1 max-w-64 px-2 py-0.5 rounded border border-[var(--border)] bg-transparent text-2xs text-[var(--text)]"
          placeholder="搜索已加载记录…"
          value={query()}
          onInput={(e) => onSearchInput(e.currentTarget.value)}
        />
        <Show when={selection()}>
          <button
            class="pressable text-2xs text-[var(--accent-hover)]"
            onClick={() => setSelection(undefined)}
          >
            清除时间选区
          </button>
        </Show>
      </div>

      <TrajectoryOverview records={ranged} selection={selection} onSelect={setSelection} />

      <Show when={loadErr()}>
        <div class="px-4 py-2 text-xs text-[var(--err)]">加载会话失败:{loadErr()}</div>
      </Show>

      <div class="flex-1 min-h-0 flex">
        <Show
          when={visibleTurns().length > 0}
          fallback={
            <div class="flex-1 min-w-0 px-4 py-6 text-xs text-[var(--text-faint)]">
              {loadErr() ? "" : "无记录"}
            </div>
          }
        >
          <TrajectoryList
            turns={visibleTurns}
            collapseTurns={collapseTurns}
            collapseCalls={collapseCalls}
            showDuration={showDuration}
            expandedTurns={expandedTurns}
            onToggleTurn={toggleTurn}
            selectedIndex={selectedIndex}
            onSelect={(index) => setSelectedIndex(selectedIndex() === index ? undefined : index)}
            hasEarlier={hasEarlier}
            earlierLabel={earlierLabel}
            onLoadEarlier={() => setLimit((current) => current + PAGE)}
            focusIndex={focusIndex}
          />
        </Show>
        <TrajectoryInspector record={selectedRecord} onClose={() => setSelectedIndex(undefined)} />
      </div>
    </div>
  );
}
