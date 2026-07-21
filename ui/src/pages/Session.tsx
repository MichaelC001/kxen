import { createSignal, For, Show, onCleanup, onMount } from "solid-js";
import { onLlmDelta, sendMessage } from "../lib/chat";
import Markdown from "../components/Markdown";
import ToolCard from "../components/ToolCard";

interface MsgItem {
  kind: "msg";
  role: "user" | "assistant";
  content: string;
  reasoning?: string;
  usage?: { input: number; output: number };
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

export default function Session() {
  const [items, setItems] = createSignal<Item[]>([]);
  const [draft, setDraft] = createSignal("");
  const [streaming, setStreaming] = createSignal(false);
  let unlisten: (() => void) | undefined;
  let listRef: HTMLDivElement | undefined;

  const scroll = () => queueMicrotask(() => listRef && (listRef.scrollTop = listRef.scrollHeight));

  /** 追加到最后一条 assistant 消息（没有则新建）。 */
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
      (text) => appendAssistant("content", text),
      (reasoning) => appendAssistant("reasoning", reasoning),
      (usage, error) => {
        setItems((prev) => {
          const last = prev[prev.length - 1];
          if (last?.kind === "msg" && last.role === "assistant") {
            return [...prev.slice(0, -1), { ...last, usage, error }];
          }
          return prev;
        });
        setStreaming(false);
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

  const send = async () => {
    const text = draft().trim();
    if (!text || streaming()) return;
    setDraft("");
    setStreaming(true);
    const history = items()
      .filter((i): i is MsgItem => i.kind === "msg")
      .map((m) => ({ role: m.role, content: m.content }));
    setItems((prev) => [...prev, { kind: "msg", role: "user", content: text }]);
    scroll();
    await sendMessage(text, history);
  };

  return (
    <div class="h-full flex flex-col">
      <div class="material px-4 py-2 border-b border-[var(--border)] text-xs text-[var(--text-dim)] flex items-center gap-3">
        <span class="font-medium">会话</span>
        <Show when={streaming()}>
          <span class="text-[var(--accent-hover)]">进行中…</span>
        </Show>
      </div>

      <div ref={(el) => (listRef = el)} class="flex-1 overflow-auto px-4 py-4">
        <div class="max-w-3xl mx-auto space-y-3">
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
              return (
                <div class={item.role === "user" ? "flex justify-end" : "flex justify-start"}>
                  <div
                    class="max-w-[85%] rounded-lg px-3.5 py-2.5 text-sm"
                    classList={{
                      "bg-[var(--accent)] text-white": item.role === "user",
                      "bg-[var(--bg-raised)] border border-[var(--border)]":
                        item.role === "assistant",
                    }}
                  >
                    <Show when={item.reasoning}>
                      <div class="text-xs text-[var(--text-faint)] border-l-2 border-[var(--border)] pl-2 mb-2 whitespace-pre-wrap">
                        {item.reasoning}
                      </div>
                    </Show>
                    <Show when={item.role === "user"} fallback={<Markdown text={item.content} />}>
                      <span class="whitespace-pre-wrap">{item.content}</span>
                    </Show>
                    <Show when={item.usage}>
                      <div class="text-[10px] text-[var(--text-faint)] mt-1.5">
                        in {item.usage!.input} / out {item.usage!.output}
                      </div>
                    </Show>
                    <Show when={item.error}>
                      <div class="text-xs text-[var(--err)] mt-1.5">{item.error}</div>
                    </Show>
                  </div>
                </div>
              );
            }}
          </For>

          <Show when={items().length === 0}>
            <div class="text-center mt-24 space-y-3">
              <div class="text-[var(--text-dim)]">发一条消息开始</div>
              <div class="text-xs text-[var(--text-faint)] space-y-1">
                <div>write-goal：定义一个带完成判据的目标</div>
                <div>tool_search：按需发现工具（todo / webfetch）</div>
                <div>workflow：让我自己写编排脚本并行派发子代理</div>
              </div>
            </div>
          </Show>
        </div>
      </div>

      <div class="p-3 border-t border-[var(--border)]">
        <div class="max-w-3xl mx-auto flex gap-2">
          <textarea
            class="flex-1 bg-[var(--bg-raised)] border border-[var(--border)] rounded-md px-3 py-2 text-sm resize-none focus:outline-none focus:border-[var(--accent)] placeholder:text-[var(--text-faint)]"
            rows={2}
            placeholder="输入消息，Enter 发送（Shift+Enter 换行）"
            value={draft()}
            onInput={(e) => setDraft(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                void send();
              }
            }}
          />
          <button
            class="pressable px-4 rounded-md bg-[var(--accent)] hover:bg-[var(--accent-hover)] text-sm text-white disabled:opacity-40"
            onClick={() => void send()}
            disabled={streaming() || !draft().trim()}
          >
            发送
          </button>
        </div>
      </div>
    </div>
  );
}
