import { createEffect, createSignal, For, Show, onCleanup, onMount } from "solid-js";
import {
  currentModel,
  onLlmDelta,
  sendMessage,
  sessionAbort,
  sessionExport,
  sessionFork,
  sessionMessages,
  statusline,
  type ContextItem,
} from "../lib/chat";
import MessageActions from "../components/MessageActions";
import {
  activeSessionId,
  ensureActiveSession,
  newSession,
  refreshSessions,
  sessions,
  setHasConversation,
  switchSession,
} from "../lib/state";
import { onDragStart } from "../lib/drag";
import Markdown from "../components/Markdown";
import ThinkingOrb from "../components/ThinkingOrb";
import type { OrbState } from "../lib/orb";
import EmptyHero from "../components/EmptyHero";
import ToolCard from "../components/ToolCard";
import Composer from "../components/composer/LexicalComposer";
import { Download, FolderOpen } from "lucide-solid";
import { toItems, type Item } from "../lib/items";

export default function Session() {
  const [items, setItems] = createSignal<Item[]>([]);
  const [streamingSid, setStreamingSid] = createSignal("");
  const [orbPhase, setOrbPhase] = createSignal<OrbState>("thinking");
  const [focusTick, setFocusTick] = createSignal(0);
  const [workdir, setWorkdir] = createSignal("");
  let unlisten: (() => void) | undefined;
  let listRef: HTMLDivElement | undefined;

  const streaming = () => streamingSid() === activeSessionId() && activeSessionId() !== "";
  const title = () =>
    activeSessionId() === ""
      ? "新会话"
      : (sessions().find((s) => s.id === activeSessionId())?.title ?? "会话");
  const scroll = () => queueMicrotask(() => listRef && (listRef.scrollTop = listRef.scrollHeight));

  // 有对话内容才驱动右 dock 滑入
  createEffect(() => setHasConversation(items().length > 0));

  // 切换会话：加载存储的时间线；草稿态（""）清空
  createEffect(() => {
    const id = activeSessionId();
    setFocusTick((t) => t + 1);
    if (!id) {
      setItems([]);
      return;
    }
    void sessionMessages(id).then((messages) => {
      if (activeSessionId() === id) {
        setItems(toItems(messages));
        scroll();
      }
    });
  });

  const appendAssistant = (field: "content" | "reasoning", text: string) => {
    setOrbPhase("composing");
    setItems((prev) => {
      const last = prev.at(-1);
      if (last?.kind === "msg" && last.role === "assistant") {
        return [...prev.slice(0, -1), { ...last, [field]: (last[field] ?? "") + text }];
      }
      return [
        ...prev,
        {
          kind: "msg",
          role: "assistant",
          content: field === "content" ? text : "",
          reasoning: field === "reasoning" ? text : undefined,
        },
      ];
    });
    scroll();
  };

  onMount(async () => {
    const sl = await statusline("").catch(() => null);
    if (sl) setWorkdir(sl.workdir);
    const m = await currentModel().catch(() => null);
    if (m) setModelLabel(`${m.provider}/${m.model}`);
    unlisten = await onLlmDelta(
      activeSessionId,
      (text) => appendAssistant("content", text),
      (reasoning) => appendAssistant("reasoning", reasoning),
      (stats, error) => {
        setOrbPhase(error ? "error" : "thinking");
        setItems((prev) => {
          const last = prev.at(-1);
          if (last?.kind === "msg" && last.role === "assistant") {
            return [...prev.slice(0, -1), { ...last, stats, error }];
          }
          return prev;
        });
        setStreamingSid("");
        scroll();
      },
      (event) => {
        if (event.kind === "tool_call") {
          setOrbPhase("searching");
          setItems((prev) => [
            ...prev,
            { kind: "tool", name: event.name, call: event.summary ?? "" },
          ]);
        } else if (event.kind === "tool_result") {
          setItems((prev) => {
            for (let i = prev.length - 1; i >= 0; i--) {
              const item = prev[i];
              if (!item) continue;
              if (item.kind === "tool" && item.name === event.name && item.result === undefined) {
                const next = [...prev];
                next[i] = { ...item, result: event.summary ?? "" };
                return next;
              }
            }
            return prev;
          });
        } else {
          setItems((prev) => [...prev, { kind: "phase", name: event.name }]);
        }
        scroll();
      },
    );
  });

  onCleanup(() => unlisten?.());

  const send = async (
    text: string,
    context: ContextItem[],
    images: Array<{ media_type: string; data: string }>,
  ) => {
    if (streaming()) return;
    // 草稿态首条消息：此时才落库成会话
    const sid = await ensureActiveSession();
    setStreamingSid(sid);
    setOrbPhase("thinking");
    setItems((prev) => [...prev, { kind: "msg", role: "user", content: text }]);
    scroll();
    await sendMessage(sid, text, context, images);
  };

  const stop = () => {
    const sid = activeSessionId();
    if (sid) void sessionAbort(sid);
  };

  /** 从指定消息分叉：新会话带前缀历史并切入。 */
  const forkAt = async (messageId: string) => {
    const forked = await sessionFork(activeSessionId(), messageId).catch(() => null);
    if (forked) {
      await refreshSessions();
      switchSession(forked.id);
    }
  };

  const [exportNote, setExportNote] = createSignal("");
  const [modelLabel, setModelLabel] = createSignal("");
  const doExport = async () => {
    const r = await sessionExport(activeSessionId()).catch(() => null);
    setExportNote(r ? `已导出 ${r.path}` : "导出失败");
    setTimeout(() => setExportNote(""), 3000);
  };

  /** 重新生成：把该 assistant 之前最近一条 user 消息重发一次。 */
  const rerun = async (idx: number) => {
    const list = items();
    for (let j = idx - 1; j >= 0; j--) {
      const m = list[j];
      if (m?.kind === "msg" && m.role === "user") {
        await send(m.content, [], []);
        return;
      }
    }
  };

  /** 编辑重发：fork 到该消息前一条（排除本消息），再发编辑后的文本。 */
  const editResend = async (idx: number, text: string) => {
    const list = items();
    for (let j = idx - 1; j >= 0; j--) {
      const m = list[j];
      if (m?.kind === "msg" && m.messageId) {
        const forked = await sessionFork(activeSessionId(), m.messageId).catch(() => null);
        if (forked) {
          await refreshSessions();
          switchSession(forked.id);
          await send(text, [], []);
          return;
        }
      }
    }
    // 没有更早消息（首条）：直接新会话发送
    await newSession();
    await send(text, [], []);
  };

  return (
    <div class="h-full flex-1 min-w-0 flex flex-col">
      <div
        class="material px-4 py-2.5 border-b border-[var(--border)] text-xs flex items-center gap-3"
        data-tauri-drag-region
        onMouseDown={onDragStart}
      >
        <span class="font-medium text-[var(--text)] truncate">{title()}</span>
        <span
          class="flex items-center gap-1 text-[var(--text-faint)] truncate popup-detail"
          title={workdir()}
        >
          <FolderOpen size={12} />
          <span class="truncate">{workdir()}</span>
        </span>
        <Show when={streaming()}>
          <span class="inline-flex items-center gap-1.5 text-[var(--accent-hover)]">
            <ThinkingOrb state={orbPhase} size={20} />
            {orbPhase() === "thinking" && "思考中"}
            {orbPhase() === "searching" && "检索中"}
            {orbPhase() === "composing" && "生成中"}
            {orbPhase() === "error" && "出错"}
          </span>
        </Show>
        <span class="ml-auto flex items-center gap-1">
          <Show when={exportNote()}>
            <span class="text-2xs text-[var(--ok)]">{exportNote()}</span>
          </Show>
          <button
            class="pressable px-1.5 py-1 rounded text-[var(--text-faint)] hover:text-[var(--text)]"
            title="导出会话为 markdown"
            onClick={() => void doExport()}
          >
            <Download size={13} />
          </button>
        </span>
      </div>

      <div ref={(el) => (listRef = el)} class="flex-1 overflow-auto px-4 py-5">
        <div class="w-full space-y-4">
          <For each={items()}>
            {(item, i) => {
              if (item.kind === "tool") {
                return <ToolCard name={item.name} call={item.call} result={item.result} />;
              }
              if (item.kind === "phase") {
                return (
                  <div class="text-xs text-[var(--text-faint)] flex items-center gap-2">
                    <span class="inline-block w-1 h-1 rounded-full bg-[var(--accent)]" />
                    phase: {item.name}
                  </div>
                );
              }
              if (item.role === "user") {
                return (
                  <div class="group relative flex flex-col items-end gap-1">
                    <div class="selectable max-w-[80%] rounded-2xl rounded-br-md px-3.5 py-2 text-sm bg-[var(--accent)] text-[var(--accent-contrast)] whitespace-pre-wrap">
                      {item.content}
                    </div>
                    <Show when={item.messageId}>
                      <div class="self-end">
                        <MessageActions
                          role="user"
                          content={item.content}
                          onFork={() => void forkAt(item.messageId!)}
                          onEditResend={(text) => void editResend(i(), text)}
                        />
                      </div>
                    </Show>
                  </div>
                );
              }
              // assistant：全宽排版，无气泡（现代 agent UI 形态）
              return (
                <div class="group relative text-sm">
                  <Show when={item.messageId}>
                    <div class="absolute right-0 top-0 z-10">
                      <MessageActions
                        role="assistant"
                        content={item.content}
                        onFork={() => void forkAt(item.messageId!)}
                        onRerun={() => void rerun(i())}
                      />
                    </div>
                  </Show>
                  <Show when={item.reasoning}>
                    <div class="selectable text-xs text-[var(--text-faint)] border-l-2 border-[var(--border)] pl-2.5 mb-2 whitespace-pre-wrap">
                      {item.reasoning}
                    </div>
                  </Show>
                  <Markdown text={item.content} />
                  <Show when={item.stats}>
                    {(stats) => (
                      <div class="text-2xs text-[var(--text-faint)] mt-1.5 tabular-nums">
                        <Show when={modelLabel()}>
                          <span class="text-[var(--text-dim)]">{modelLabel()}</span> ·{" "}
                        </Show>
                        in {stats().input_tokens} / out {stats().output_tokens} · TTFT{" "}
                        {(stats().ttft_ms / 1000).toFixed(1)}s ·{" "}
                        {(stats().duration_ms / 1000).toFixed(1)}s · {stats().tokens_per_sec} tok/s
                      </div>
                    )}
                  </Show>
                  <Show when={item.error}>
                    <div class="text-xs text-[var(--err)] mt-1.5 flex items-center gap-2">
                      {item.error}
                      <Show when={item.error === "(已中断)" && !streaming()}>
                        <button
                          class="pressable px-2 py-0.5 rounded text-2xs border border-[var(--border)] text-[var(--text-dim)]"
                          onClick={() => void send("继续", [], [])}
                        >
                          继续
                        </button>
                      </Show>
                    </div>
                  </Show>
                </div>
              );
            }}
          </For>

          <Show when={items().length === 0}>
            <EmptyHero />
          </Show>
        </div>
      </div>

      <div class="px-3 pb-3 composer-fade">
        <div class="w-full">
          <Composer
            streaming={streaming}
            onSend={(t, c, i) => void send(t, c, i)}
            onStop={stop}
            focusTick={focusTick}
          />
        </div>
      </div>
    </div>
  );
}
