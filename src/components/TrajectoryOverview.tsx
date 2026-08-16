// Overview 时间线：钉在记录表上方的计时投影。已加载记录按真实起止投影为时间条；
// assistant 条以首个非空 token 为界区分 TTFT 与解码（不同色按比例），计时不完整退化单一颜色。
// 交互对齐 Chrome DevTools Network：左键拖选聚焦区间 / 滚轮缩放 / 右键单击清除 / 右键拖动平移 /
// 悬停 500ms 出精确起止/总耗时/TTFT/解码 tooltip。无计时数据的记录不投影，绝不虚构耗时。
import { createMemo, createSignal, For, onCleanup, onMount, Show, type Accessor } from "solid-js";
import {
  formatClock,
  formatMs,
  overviewBars,
  type OverviewBar,
  type TrajectoryRecord,
} from "../lib/trajectory";

export interface TimeRange {
  start: number;
  end: number;
}

export default function TrajectoryOverview(props: {
  records: Accessor<TrajectoryRecord[]>;
  selection: Accessor<TimeRange | undefined>;
  onSelect: (range: TimeRange | undefined) => void;
}) {
  const bars = createMemo(() => overviewBars(props.records()));
  const extent = createMemo((): TimeRange | undefined => {
    const all = bars();
    if (!all.length) return undefined;
    return { start: Math.min(...all.map((b) => b.start)), end: Math.max(...all.map((b) => b.end)) };
  });
  // 可视时间域（缩放/平移改变）；数据 extent 变化且用户未缩放过时跟随
  const [domain, setDomain] = createSignal<TimeRange | undefined>(undefined);
  const view = () => domain() ?? extent();
  const [hover, setHover] = createSignal<{ bar: OverviewBar; x: number; y: number } | undefined>();
  const [pending, setPending] = createSignal<TimeRange | undefined>();
  let track: HTMLDivElement | undefined;
  let hoverTimer: ReturnType<typeof setTimeout> | undefined;
  let drag: { button: number; startX: number; domainStart: number; moved: boolean } | undefined;

  const span = () => {
    const v = view();
    return v ? Math.max(1, v.end - v.start) : 1;
  };
  const pct = (t: number) => {
    const v = view();
    return v ? ((t - v.start) / span()) * 100 : 0;
  };
  const timeAt = (clientX: number) => {
    const rect = track?.getBoundingClientRect();
    const v = view();
    if (!rect || !v) return 0;
    return v.start + (Math.min(Math.max(clientX - rect.left, 0), rect.width) / rect.width) * span();
  };

  const clearHover = () => {
    if (hoverTimer) clearTimeout(hoverTimer);
    hoverTimer = undefined;
    setHover(undefined);
  };

  const onPointerDown = (e: PointerEvent) => {
    const v = view();
    if (!v) return;
    // 合成指针事件（测试/无障碍输入）没有活动指针，捕获失败不影响拖选
    try {
      track?.setPointerCapture(e.pointerId);
    } catch {
      /* noop */
    }
    drag = { button: e.button, startX: e.clientX, domainStart: v.start, moved: false };
    if (e.button === 0) setPending(undefined);
  };
  const onPointerMove = (e: PointerEvent) => {
    if (drag) {
      const dx = e.clientX - drag.startX;
      if (Math.abs(dx) > 3) drag.moved = true;
      if (drag.button === 0 && drag.moved) {
        const a = timeAt(drag.startX);
        const b = timeAt(e.clientX);
        setPending({ start: Math.min(a, b), end: Math.max(a, b) });
      } else if (drag.button === 2 && drag.moved) {
        const rect = track?.getBoundingClientRect();
        if (!rect) return;
        const dt = (-dx / rect.width) * span();
        setDomain({ start: drag.domainStart + dt, end: drag.domainStart + dt + span() });
      }
      return;
    }
    // 悬停 500ms 出 tooltip
    const target = (e.target as HTMLElement).closest("[data-bar]") as HTMLElement | null;
    if (!target) {
      clearHover();
      return;
    }
    const index = Number(target.dataset["bar"]);
    const bar = bars()[index];
    if (!bar || hover()?.bar === bar) return;
    clearHover();
    hoverTimer = setTimeout(() => setHover({ bar, x: e.clientX, y: e.clientY }), 500);
  };
  const onPointerUp = () => {
    const current = drag;
    drag = undefined;
    if (!current) return;
    if (current.button === 0 && current.moved && pending()) {
      props.onSelect(pending());
      setPending(undefined);
    } else if (current.button === 2 && !current.moved) {
      props.onSelect(undefined); // 右键单击清除选区，恢复全表
    }
    setPending(undefined);
  };
  const onWheel = (e: WheelEvent) => {
    const v = view();
    if (!v) return;
    e.preventDefault();
    const center = timeAt(e.clientX);
    const factor = e.deltaY > 0 ? 1.2 : 1 / 1.2;
    const next = Math.min(Math.max(span() * factor, 10), 24 * 3_600_000);
    const ratio = (center - v.start) / span();
    setDomain({ start: center - next * ratio, end: center + next * (1 - ratio) });
  };

  onMount(() => track?.addEventListener("wheel", onWheel, { passive: false }));
  onCleanup(() => clearHover());

  const shown = () => pending() ?? props.selection();
  return (
    <div data-testid="trajectory-overview" class="px-4 pt-2 pb-1 select-none">
      <Show
        when={view()}
        fallback={
          <div class="text-2xs text-[var(--text-faint)] py-2">
            无计时数据（本页记录均未落盘起止/统计）
          </div>
        }
      >
        <div
          ref={track}
          class="relative h-10 rounded border border-[var(--border)] bg-[var(--bg-raised)] overflow-hidden cursor-crosshair"
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerLeave={clearHover}
          onContextMenu={(e) => e.preventDefault()}
        >
          <For each={bars()}>
            {(bar, index) => {
              const left = () => pct(bar.start);
              const width = () => Math.max(0.5, pct(bar.end) - left());
              const ttftWidth = () =>
                bar.ttftMs !== undefined
                  ? Math.min(width(), (bar.ttftMs / Math.max(1, bar.end - bar.start)) * width())
                  : undefined;
              return (
                <div
                  data-bar={index()}
                  class="absolute top-1 bottom-1 rounded-sm"
                  classList={{
                    "bg-[var(--accent)]/70": bar.kind === "message",
                    "bg-[var(--warn)]/70": bar.kind === "tool",
                  }}
                  style={{ left: `${left()}%`, width: `${width()}%` }}
                  title=""
                >
                  <Show when={ttftWidth() !== undefined}>
                    <div
                      class="absolute left-0 top-0 bottom-0 rounded-l-sm bg-[var(--accent)]"
                      style={{ width: `${ttftWidth()}%` }}
                    />
                  </Show>
                </div>
              );
            }}
          </For>
          <Show when={shown()}>
            {(range) => (
              <div
                data-testid="overview-selection"
                class="absolute top-0 bottom-0 border-x border-[var(--accent)] bg-[var(--accent)]/10 pointer-events-none"
                style={{
                  left: `${pct(range().start)}%`,
                  width: `${Math.max(0.2, pct(range().end) - pct(range().start))}%`,
                }}
              />
            )}
          </Show>
        </div>
        <div class="flex justify-between text-2xs text-[var(--text-faint)] tabular-nums mt-0.5">
          <span>{formatClock(view()!.start)}</span>
          <span>{formatMs(span())}</span>
          <span>{formatClock(view()!.end)}</span>
        </div>
      </Show>
      <Show when={hover()}>
        {(h) => (
          <div
            data-testid="overview-tooltip"
            class="fixed z-30 rounded border border-[var(--border)] bg-[var(--bg-raised)] px-2 py-1 text-2xs text-[var(--text-dim)] pointer-events-none"
            style={{ left: `${h().x + 8}px`, top: `${h().y + 8}px` }}
          >
            <div>{h().bar.label}</div>
            <div>
              {formatClock(h().bar.start)} - {formatClock(h().bar.end)} · 总耗时{" "}
              {formatMs(h().bar.end - h().bar.start)}
            </div>
            <Show when={h().bar.ttftMs !== undefined}>
              <div>
                TTFT {formatMs(h().bar.ttftMs!)} · 解码{" "}
                {formatMs(Math.max(0, h().bar.end - h().bar.start - h().bar.ttftMs!))}
              </div>
            </Show>
          </div>
        )}
      </Show>
    </div>
  );
}
