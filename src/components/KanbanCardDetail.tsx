// 看板卡片详情：标题/正文/状态/评论流 + 人工动作（待审卡的通过/打回、阻塞卡的重试、任意卡评论）。
// 纯展示组件：动作经 props 回调上抛，页面统一走 act()（pending 禁用 + flashErr + 重拉）。
import { createSignal, For, Show } from "solid-js";
import { Check, MessageSquare, RotateCcw, X } from "lucide-solid";
import type { KanbanCard, KanbanColumn } from "../lib/chat";
import { KANBAN_TONE_CLASS, kanbanStatusMeta } from "../lib/board";
import { relTime } from "../lib/time";

export default function KanbanCardDetail(props: {
  card: KanbanCard;
  column: KanbanColumn | undefined;
  acting: boolean;
  onClose: () => void;
  onMove: (outcome: "success" | "failure") => void;
  onRetry: () => void;
  onComment: (body: string) => void;
}) {
  const [draft, setDraft] = createSignal("");
  const meta = () => kanbanStatusMeta(props.card.status);
  // 通过/打回只在 human_gate 列停车时给出：迁移目标由列 transitions 推导，别的列没有人工语义
  const gated = () => props.card.status === "waiting_human" && props.column?.on_enter.kind === "human_gate";

  const submit = () => {
    const body = draft().trim();
    if (!body) return;
    setDraft("");
    props.onComment(body);
  };

  return (
    <div class="w-80 shrink-0 rounded-lg border border-[var(--border)] bg-[var(--bg-raised)] flex flex-col self-start sticky top-0">
      <div class="flex items-start gap-2 p-3 border-b border-[var(--border)]">
        <div class="flex-1 min-w-0">
          <div class="text-sm font-medium text-[var(--text)] break-words">{props.card.title}</div>
          <div class="mt-0.5 flex items-center gap-1.5 text-2xs">
            <Show when={props.card.status === "running"}>
              <span class="w-1.5 h-1.5 rounded-full bg-[var(--ok)] animate-pulse" />
            </Show>
            <span class={KANBAN_TONE_CLASS[meta().tone]}>{meta().label}</span>
            <span class="text-[var(--text-faint)]">{relTime(props.card.updated_at)}</span>
          </div>
        </div>
        <button
          class="pressable p-1 rounded text-[var(--text-faint)] hover:text-[var(--text)]"
          title="关闭详情"
          onClick={() => props.onClose()}
        >
          <X size={13} />
        </button>
      </div>

      <div class="p-3 space-y-3 overflow-y-auto">
        <Show when={props.card.body.trim()}>
          <p class="text-xs text-[var(--text-dim)] whitespace-pre-wrap break-words selectable">
            {props.card.body}
          </p>
        </Show>
        <Show when={props.card.block_reason}>
          {(reason) => (
            <div class="rounded border border-[var(--err)]/50 bg-[var(--err)]/5 px-2 py-1.5 text-2xs text-[var(--err)] break-words">
              {reason()}
            </div>
          )}
        </Show>

        <div class="flex items-center gap-2">
          <Show when={gated()}>
            <button
              class="pressable px-2.5 py-1 rounded border border-[var(--ok)]/60 text-xs text-[var(--ok)] flex items-center gap-1 disabled:opacity-40"
              disabled={props.acting}
              onClick={() => props.onMove("success")}
            >
              <Check size={11} />
              通过
            </button>
            <button
              class="pressable px-2.5 py-1 rounded border border-[var(--err)]/60 text-xs text-[var(--err)] flex items-center gap-1 disabled:opacity-40"
              disabled={props.acting}
              onClick={() => props.onMove("failure")}
            >
              <X size={11} />
              打回
            </button>
          </Show>
          <Show when={props.card.status === "blocked"}>
            <button
              class="pressable px-2.5 py-1 rounded border border-[var(--border)] text-xs text-[var(--text)] flex items-center gap-1 disabled:opacity-40"
              disabled={props.acting}
              title="落 run_started 事件，runner 收养后自动重跑"
              onClick={() => props.onRetry()}
            >
              <RotateCcw size={11} />
              重试
            </button>
          </Show>
        </div>

        <div>
          <div class="text-2xs uppercase tracking-wider text-[var(--text-faint)] mb-1">
            评论（{props.card.comments.length}）
          </div>
          <For each={props.card.comments} fallback={<div class="text-2xs text-[var(--text-faint)]">还没有评论</div>}>
            {(comment) => (
              <div class="py-1.5 border-t border-[var(--border)] first:border-t-0">
                <div class="flex items-center gap-1.5 text-2xs">
                  <span
                    class={comment.author === "agent" ? "text-[var(--text-faint)]" : "text-[var(--text)]"}
                  >
                    {comment.author}
                  </span>
                  <span class="text-[var(--text-faint)]">{relTime(comment.at)}</span>
                </div>
                <p class="text-xs text-[var(--text-dim)] whitespace-pre-wrap break-words selectable mt-0.5">
                  {comment.body}
                </p>
              </div>
            )}
          </For>
          <div class="mt-2 flex items-start gap-1.5">
            <textarea
              class="flex-1 px-2 py-1.5 rounded border border-[var(--border)] bg-transparent text-xs resize-none"
              rows={2}
              placeholder="写下评论…"
              value={draft()}
              onInput={(e) => setDraft(e.currentTarget.value)}
            />
            <button
              class="pressable px-2 py-1.5 rounded border border-[var(--border)] text-xs flex items-center gap-1 disabled:opacity-40"
              disabled={props.acting || !draft().trim()}
              onClick={submit}
            >
              <MessageSquare size={11} />
              评论
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
