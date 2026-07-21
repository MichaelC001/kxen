import { createEffect, createSignal, For, Show, onCleanup, onMount } from "solid-js";
import {
  onLlmDelta,
  sendMessage,
  sessionAbort,
  sessionMessages,
  type ContextItem,
  type RunStats,
  type StoredMessage,
} from "../lib/chat";
import { activeSessionId, ensureActiveSession, sessions, setHasConversation } from "../lib/state";
import Markdown from "../components/Markdown";
import ToolCard from "../components/ToolCard";
import Composer from "../components/Composer";

interface MsgItem {
  kind: "msg";
  role: "user" | "assistant";
  content: string;
  reasoning?: string;
  stats?: RunStats;
  error?: string;
}
interface ToolItem {
  kind: "tool";
  name: string;
  call: string;
  result?: string;
}
interface PhaseItem {
  kind: "phase";
  name: string;
}
type Item = MsgItem | ToolItem | PhaseItem;

/** 存储消息 -> 时间线条目（工具调用/推理/文本按序还原）。 */
function toItems(messages: StoredMessage[]): Item[] {
  const items: Item[] = [];
  for (const m of messages) {
    if (m.role === "system") continue;
    for (const p of m.parts) {
      if (p.type === "text" && p.text) {
        const last = items[items.length - 1];
        if (last?.kind === "msg" && last.role === m.role) {
          items[items.length - 1] = { ...last, content: `${last.content}\n${p.text}` };
        } else {
          items.push({ kind: "msg", role: m.role, content: p.text });
        }
      } else if (p.type === "reasoning" && p.text && m.role === "assistant") {
        const last = items[items.length - 1];
        if (last?.kind === "msg" && last.role === "assistant") {
          items[items.length - 1] = { ...last, reasoning: `${last.reasoning ?? ""}${p.text}` };
        }
      } else if (p.type === "tool_call" && p.name) {
        items.push({
          kind: "tool",
          name: p.name,
          call: typeof p.input === "string" ? p.input : JSON.stringify(p.input),
          result: p.output || undefined,
        });
      }
    }
  }
  return items;
}

export default function Session() {
  const [items, setItems] = createSignal<Item[]>([]);
  const [streamingSid, setStreamingSid] = createSignal("");
  const [focusTick, setFocusTick] = createSignal(0);
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
    setItems((prev) => {
      const last = prev[prev.length - 1];
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
    unlisten = await onLlmDelta(
      activeSessionId,
      (text) => appendAssistant("content", text),
      (reasoning) => appendAssistant("reasoning", reasoning),
      (stats, error) => {
        setItems((prev) => {
          const last = prev[prev.length - 1];
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
          setItems((prev) => [
            ...prev,
            { kind: "tool", name: event.name, call: event.summary ?? "" },
          ]);
        } else if (event.kind === "tool_result") {
          setItems((prev) => {
            for (let i = prev.length - 1; i >= 0; i--) {
              const item = prev[i];
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
    setItems((prev) => [...prev, { kind: "msg", role: "user", content: text }]);
    scroll();
    await sendMessage(sid, text, context, images);
  };

  const stop = () => {
    const sid = activeSessionId();
    if (sid) void sessionAbort(sid);
  };

  return (
    <div class="h-full flex-1 min-w-0 flex flex-col">
      <div
        class="material px-4 py-2.5 border-b border-[var(--border)] text-xs flex items-center gap-3"
        data-tauri-drag-region
      >
        <span class="font-medium text-[var(--text)] truncate">{title()}</span>
        <Show when={streaming()}>
          <span class="inline-flex items-center gap-1.5 text-[var(--accent-hover)]">
            <span class="w-1.5 h-1.5 rounded-full bg-[var(--accent-hover)] animate-pulse" />
            进行中
          </span>
        </Show>
      </div>

      <div ref={(el) => (listRef = el)} class="flex-1 overflow-auto px-4 py-5">
        <div class="max-w-3xl mx-auto space-y-4">
          <For each={items()}>
            {(item) => {
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
                  <div class="flex justify-end">
                    <div class="max-w-[80%] rounded-2xl rounded-br-md px-3.5 py-2 text-sm bg-[var(--accent)] text-[var(--accent-contrast)] whitespace-pre-wrap">
                      {item.content}
                    </div>
                  </div>
                );
              }
              // assistant：全宽排版，无气泡（现代 agent UI 形态）
              return (
                <div class="text-sm">
                  <Show when={item.reasoning}>
                    <div class="text-xs text-[var(--text-faint)] border-l-2 border-[var(--border)] pl-2.5 mb-2 whitespace-pre-wrap">
                      {item.reasoning}
                    </div>
                  </Show>
                  <Markdown text={item.content} />
                  <Show when={item.stats}>
                    {(stats) => (
                      <div class="text-[10px] text-[var(--text-faint)] mt-1.5 tabular-nums">
                        in {stats().input_tokens} / out {stats().output_tokens} · TTFT{" "}
                        {(stats().ttft_ms / 1000).toFixed(1)}s ·{" "}
                        {(stats().duration_ms / 1000).toFixed(1)}s · {stats().tokens_per_sec} tok/s
                      </div>
                    )}
                  </Show>
                  <Show when={item.error}>
                    <div class="text-xs text-[var(--err)] mt-1.5">{item.error}</div>
                  </Show>
                </div>
              );
            }}
          </For>

          <Show when={items().length === 0}>
            <div class="pt-24 space-y-6">
              <div class="text-center space-y-2">
                <div class="text-[var(--text)] font-medium">发一条消息开始</div>
                <div class="text-xs text-[var(--text-faint)]">
                  @ 引用文件 · / 命令与 skill · # 沉淀知识 · 粘贴图片
                </div>
              </div>
            </div>
          </Show>
        </div>
      </div>

      <div class="p-3 border-t border-[var(--border)]">
        <div class="max-w-3xl mx-auto">
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
