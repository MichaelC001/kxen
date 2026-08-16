// composer 上下文占用指示：一枚小圆环 + 点击展开的组成明细弹层。
// 数据源 session.context_stats（ws/context_stats.rs）：三段拆分为 chars/4 估算（带 ~ 展示），
// last_input_tokens 是最近一次 run 的 provider 实测输入（精确锚点，不带 ~，无实测 = 未知）。
// 估算与实测并列展示但不对账：两者口径不同（估算按字符、实测按 provider tokenizer）。
import { createEffect, createSignal, Show, type Accessor } from "solid-js";
import { sessionContextStats, type ContextStats } from "../../lib/context-stats";
import { createExclusiveDisclosure, onClickOutside } from "../../lib/dismiss";
import { createSeqGuard } from "../../lib/async-guard";

const RADIUS = 6;
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

/** 与 StatusBar ctx 条同阈值：>70 警、>90 险。 */
function toneCls(pct: number): string {
  return pct > 90
    ? "text-[var(--err)]"
    : pct > 70
      ? "text-[var(--warn)]"
      : "text-[var(--text-faint)]";
}

export default function ContextGauge(props: {
  sessionId: Accessor<string>;
  streaming: Accessor<boolean>;
}) {
  const [stats, setStats] = createSignal<ContextStats | null>(null);
  const [loadErr, setLoadErr] = createSignal("");
  const disclosure = createExclusiveDisclosure();
  const guard = createSeqGuard();
  let rootRef: HTMLDivElement | undefined;

  const reload = async () => {
    const sid = props.sessionId();
    if (!sid) {
      setStats(null);
      setLoadErr("");
      return;
    }
    const request = guard.next();
    try {
      const next = await sessionContextStats(sid);
      if (!guard.isCurrent(request) || props.sessionId() !== sid) return;
      setStats(next);
      setLoadErr("");
    } catch (error) {
      if (!guard.isCurrent(request) || props.sessionId() !== sid) return;
      setLoadErr(error instanceof Error ? error.message : String(error));
    }
  };

  // 会话切换 + run 结束（streaming true->false）刷新；展开弹层时也取一次新值
  let wasStreaming = false;
  createEffect(() => {
    props.sessionId();
    const streaming = props.streaming();
    if (wasStreaming && !streaming) void reload();
    wasStreaming = streaming;
  });
  createEffect(() => {
    props.sessionId();
    void reload();
  });
  createEffect(() => {
    if (disclosure.open()) void reload();
  });

  onClickOutside(
    () => rootRef,
    () => disclosure.setOpen(false),
  );

  const estimateTotal = () => {
    const s = stats();
    return s ? s.system_tokens + s.tool_tokens + s.message_tokens : 0;
  };
  const pct = () => {
    const s = stats();
    if (!s || s.window_tokens <= 0) return 0;
    return Math.min(100, Math.round((estimateTotal() / s.window_tokens) * 100));
  };
  const rows = (): { label: string; tokens: number }[] => {
    const s = stats();
    if (!s) return [];
    return [
      { label: "系统提示词", tokens: s.system_tokens },
      { label: "工具定义", tokens: s.tool_tokens },
      { label: "对话消息", tokens: s.message_tokens },
    ];
  };

  return (
    <Show when={props.sessionId()}>
      <div ref={rootRef} class="relative">
        <button
          type="button"
          data-testid="context-gauge"
          class="pressable flex items-center gap-1"
          title="上下文占用（点击展开组成明细）"
          aria-expanded={disclosure.open()}
          onClick={() => disclosure.toggle()}
        >
          <svg width="14" height="14" viewBox="0 0 16 16" class={toneCls(pct())}>
            <circle
              cx="8"
              cy="8"
              r={RADIUS}
              fill="none"
              stroke="currentColor"
              stroke-opacity="0.2"
              stroke-width="2.5"
            />
            <circle
              cx="8"
              cy="8"
              r={RADIUS}
              fill="none"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
              stroke-dasharray={`${(pct() / 100) * CIRCUMFERENCE} ${CIRCUMFERENCE}`}
              transform="rotate(-90 8 8)"
            />
          </svg>
          <span class={`text-2xs tabular-nums ${toneCls(pct())}`}>~{pct()}%</span>
        </button>
        <Show when={disclosure.open()}>
          <div
            data-testid="context-gauge-detail"
            class="composer-popup absolute bottom-full right-0 mb-1.5 w-64 max-w-[calc(100vw-16px)] rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] z-20 px-3 py-2"
          >
            <Show
              when={stats()}
              fallback={<div class="text-xs text-[var(--text-faint)]">加载中…</div>}
            >
              {(s) => (
                <>
                  <div class="space-y-1">
                    {rows().map((row) => (
                      <div class="flex items-baseline justify-between gap-3 text-xs">
                        <span class="text-[var(--text-dim)]">{row.label}</span>
                        <span class="tabular-nums text-[var(--text-faint)]">~{row.tokens} tok</span>
                      </div>
                    ))}
                    <div class="flex items-baseline justify-between gap-3 text-xs border-t border-[var(--border)] pt-1">
                      <span class="text-[var(--text)]">合计 / 窗口</span>
                      <span class="tabular-nums text-[var(--text-dim)]">
                        ~{estimateTotal()} / {s().window_tokens} tok（~{pct()}%）
                      </span>
                    </div>
                    <div class="flex items-baseline justify-between gap-3 text-xs">
                      <span class="text-[var(--text-dim)]">最近实测输入</span>
                      <span class="tabular-nums text-[var(--text-dim)]">
                        {s().last_input_tokens ? `${s().last_input_tokens} tok（精确）` : "未知"}
                      </span>
                    </div>
                  </div>
                  <div class="mt-1.5 text-2xs text-[var(--text-faint)]">
                    估算按 chars/4 粗估（不含 MCP/动态工具），与实测口径不同，不做对账。
                  </div>
                </>
              )}
            </Show>
            <Show when={loadErr()}>
              <div class="mt-1 text-2xs text-[var(--err)]">组成明细加载失败：{loadErr()}</div>
            </Show>
          </div>
        </Show>
      </div>
    </Show>
  );
}
