// Trajectory 记录表虚拟列表：定高条目 + 上下 spacer，只挂载可视窗 + overscan 缓冲行。
// 滚动语义在此闭环：打开定位最新尾部；用户上滚暂停跟随（新记录只涨底部 spacer，不打断检视）；
// 触顶自动加载更早一页（锚点恢复视口，加载中原地禁用，顶部按钮是同语义的显式入口）；
// Inspect 联动经 focusIndex 把目标记录滚到视口中部。
import {
  createEffect,
  createMemo,
  createSignal,
  For,
  onCleanup,
  onMount,
  Show,
  type Accessor,
} from "solid-js";
import { ChevronDown, ChevronRight } from "lucide-solid";
import {
  anchorScrollTop,
  entryHeight,
  entryIndexOfRecord,
  entryTop,
  flattenTurns,
  topAnchor,
  virtualWindow,
  type VirtualEntry,
} from "../lib/trajectory-virtual";
import {
  formatMs,
  toolDurationMs,
  type TrajectoryRecord,
  type TrajectoryTurn,
} from "../lib/trajectory";

const KIND_LABEL: Record<TrajectoryRecord["kind"], string> = {
  system: "system",
  user: "user",
  context: "context",
  compacted: "compacted",
  message: "message",
  tool: "tool",
  subtool: "subtool",
};

function Row(props: {
  record: TrajectoryRecord;
  showDuration: Accessor<boolean>;
  collapseCalls: Accessor<boolean>;
  selected: Accessor<boolean>;
  onSelect: () => void;
}) {
  const duration = () => (props.record.tool ? toolDurationMs(props.record.tool) : undefined);
  return (
    <button
      type="button"
      data-testid="trajectory-row"
      data-record-index={props.record.index}
      style={{ height: "24px" }}
      class="w-full flex items-baseline gap-2 px-3 py-1 text-left text-xs overflow-hidden hover:bg-[var(--bg-overlay)] border-l-2"
      classList={{
        "bg-[var(--bg-overlay)]": props.selected(),
        "border-[var(--accent)]/40": props.record.kind === "user",
        "border-transparent": props.record.kind !== "user",
      }}
      onClick={props.onSelect}
    >
      <span class="text-[var(--text-faint)] tabular-nums shrink-0 w-10">#{props.record.index}</span>
      <span class="shrink-0 w-16 text-[var(--accent-hover)]">{KIND_LABEL[props.record.kind]}</span>
      <span class="flex-1 min-w-0 truncate text-[var(--text-dim)]">
        {props.collapseCalls() && props.record.kind === "tool"
          ? (props.record.tool?.name ?? props.record.summary)
          : props.record.summary}
      </span>
      <Show when={props.showDuration()}>
        <span class="shrink-0 tabular-nums text-[var(--text-faint)]">
          {duration() !== undefined
            ? formatMs(duration()!)
            : props.record.stats
              ? formatMs(props.record.stats.duration_ms)
              : ""}
        </span>
      </Show>
    </button>
  );
}

export default function TrajectoryList(props: {
  turns: Accessor<TrajectoryTurn[]>;
  collapseTurns: Accessor<boolean>;
  collapseCalls: Accessor<boolean>;
  showDuration: Accessor<boolean>;
  expandedTurns: Accessor<Set<number>>;
  onToggleTurn: (turn: TrajectoryTurn) => void;
  selectedIndex: Accessor<number | undefined>;
  onSelect: (index: number) => void;
  hasEarlier: Accessor<boolean>;
  earlierLabel: Accessor<string>;
  onLoadEarlier: () => void;
  /** Inspect 联动的目标记录 index；父级负责扩窗保证记录已进入条目序列 */
  focusIndex: Accessor<number | undefined>;
}) {
  const [scrollTop, setScrollTop] = createSignal(0);
  const [viewport, setViewport] = createSignal(0);
  const [loadingEarlier, setLoadingEarlier] = createSignal(false);
  let listRef: HTMLDivElement | undefined;
  let follow = true;
  let anchor: { key: string; offset: number } | undefined;

  const collapsed = (t: TrajectoryTurn) =>
    props.collapseTurns() && !props.expandedTurns().has(t.startIndex);
  const entries = createMemo(() => flattenTurns(props.turns(), collapsed, props.collapseTurns()));
  const win = createMemo(() => virtualWindow(entries(), scrollTop(), viewport()));

  const atBottom = () =>
    !listRef || listRef.scrollHeight - listRef.scrollTop - listRef.clientHeight < 24;

  // 跟随尾部：条目变化时只有用户未上滚才停留尾部（初载即定位最新尾部）
  createEffect(() => {
    entries();
    if (!follow) return;
    queueMicrotask(() => {
      if (listRef) listRef.scrollTop = listRef.scrollHeight;
    });
  });

  // 加载更早的锚点恢复：prepend 后视口内容原位不动；锚点消失则停在顶部
  createEffect(() => {
    const current = entries();
    if (!anchor) return;
    const a = anchor;
    anchor = undefined;
    queueMicrotask(() => {
      if (!listRef) return;
      listRef.scrollTop = anchorScrollTop(current, a.key, a.offset);
      // 同步刷新窗口信号：不等原生 scroll 事件，prepend 后的可视窗立即重算
      setScrollTop(listRef.scrollTop);
    });
    setLoadingEarlier(false);
  });

  const loadEarlier = () => {
    if (loadingEarlier() || !props.hasEarlier()) return;
    anchor = topAnchor(entries(), listRef?.scrollTop ?? 0);
    setLoadingEarlier(true);
    props.onLoadEarlier();
    // 空表兜底：无锚点可恢复时直接解除禁用
    if (!anchor) setLoadingEarlier(false);
  };

  const onScroll = () => {
    if (!listRef) return;
    setScrollTop(listRef.scrollTop);
    follow = atBottom();
    if (listRef.scrollTop < 40 && props.hasEarlier()) loadEarlier();
  };

  onMount(() => {
    if (!listRef) return;
    setViewport(listRef.clientHeight);
    const observer = new ResizeObserver(() => {
      if (listRef) setViewport(listRef.clientHeight);
    });
    observer.observe(listRef);
    onCleanup(() => observer.disconnect());
  });

  // Inspect 联动定位：目标记录滚到视口中部，同时脱离跟随尾部
  createEffect(() => {
    const target = props.focusIndex();
    if (target === undefined) return;
    const current = entries();
    const index = entryIndexOfRecord(current, target);
    if (index < 0) return;
    follow = false;
    queueMicrotask(() => {
      if (!listRef) return;
      listRef.scrollTop = Math.max(
        0,
        entryTop(current, index) - (listRef.clientHeight - entryHeight(current[index]!)) / 2,
      );
      setScrollTop(listRef.scrollTop);
    });
  });

  const renderEntry = (entry: VirtualEntry) => {
    if (entry.type === "sep") {
      // 9px 足迹全部走 style height（margin 不计入定高换算），分隔线在带内垂直居中
      return (
        <div style={{ height: "9px" }} class="mx-3 flex items-center">
          <div class="w-full border-t border-[var(--border)]" />
        </div>
      );
    }
    if (entry.type === "turn-collapsed") {
      const turn = entry.turn;
      return (
        <button
          type="button"
          data-testid="trajectory-turn-collapsed"
          style={{ height: "24px" }}
          class="w-full flex items-center gap-2 px-3 py-1 text-left text-xs overflow-hidden text-[var(--text-faint)] hover:bg-[var(--bg-overlay)]"
          onClick={() => props.onToggleTurn(turn)}
        >
          <ChevronRight size={11} />
          <span class="truncate">{turn.records[0]?.summary ?? ""}</span>
          <span class="shrink-0 tabular-nums">
            {turn.records.length} 步骤 · {turn.toolCalls} 工具调用
          </span>
        </button>
      );
    }
    if (entry.type === "turn-head") {
      const turn = entry.turn;
      return (
        <button
          type="button"
          style={{ height: "18px" }}
          class="w-full flex items-center gap-1 px-3 overflow-hidden text-2xs text-[var(--text-faint)]"
          onClick={() => props.onToggleTurn(turn)}
        >
          <ChevronDown size={11} />
          {turn.records.length} 步骤 · {turn.toolCalls} 工具调用
        </button>
      );
    }
    return (
      <Row
        record={entry.record}
        showDuration={props.showDuration}
        collapseCalls={props.collapseCalls}
        selected={() => props.selectedIndex() === entry.record.index}
        onSelect={() => props.onSelect(entry.record.index)}
      />
    );
  };

  return (
    <div
      ref={listRef}
      data-testid="trajectory-list"
      class="flex-1 min-w-0 overflow-auto py-1"
      onScroll={onScroll}
    >
      <Show when={props.hasEarlier()}>
        <div class="px-3 py-1">
          <button
            data-testid="trajectory-load-earlier"
            class="pressable w-full px-2 py-1 rounded border border-[var(--border)] text-2xs text-[var(--text-dim)] hover:bg-[var(--bg-overlay)] disabled:opacity-50"
            disabled={loadingEarlier()}
            onClick={loadEarlier}
          >
            {loadingEarlier() ? "加载更早记录…" : props.earlierLabel()}
          </button>
        </div>
      </Show>
      <div style={{ height: `${win().topPad}px` }} />
      <For each={entries().slice(win().start, win().end)}>{renderEntry}</For>
      <div style={{ height: `${win().bottomPad}px` }} />
    </div>
  );
}
