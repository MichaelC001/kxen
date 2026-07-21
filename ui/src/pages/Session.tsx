import { createSignal, For, Show, onCleanup, onMount } from "solid-js";
import { currentModel, onLlmDelta, sendMessage, type ChatMessage } from "../lib/chat";

interface ToolActivity {
  name: string;
  call: string;
  result?: string;
}

export default function Session() {
  const [messages, setMessages] = createSignal<ChatMessage[]>([]);
  const [tools, setTools] = createSignal<ToolActivity[]>([]);
  const [draft, setDraft] = createSignal("");
  const [streaming, setStreaming] = createSignal(false);
  const [model, setModel] = createSignal("");
  let unlisten: (() => void) | undefined;
  let listRef: HTMLDivElement | undefined;

  onMount(async () => {
    const m = await currentModel();
    setModel(`${m.provider}/${m.model}`);
    unlisten = await onLlmDelta(
      (text) => {
        setMessages((prev) => {
          const last = prev[prev.length - 1];
          if (last?.role === "assistant" && !last.usage) {
            return [...prev.slice(0, -1), { ...last, content: last.content + text }];
          }
          return prev;
        });
        queueMicrotask(() => listRef && (listRef.scrollTop = listRef.scrollHeight));
      },
      (reasoning) => {
        setMessages((prev) => {
          const last = prev[prev.length - 1];
          if (last?.role === "assistant" && !last.usage) {
            return [
              ...prev.slice(0, -1),
              { ...last, reasoning: (last.reasoning ?? "") + reasoning },
            ];
          }
          return prev;
        });
      },
      (usage, error) => {
        setMessages((prev) => {
          const last = prev[prev.length - 1];
          if (last?.role === "assistant") {
            return [...prev.slice(0, -1), { ...last, usage, error }];
          }
          return prev;
        });
        setStreaming(false);
      },
      (event) => {
        if (event.kind === "tool_call") {
          setTools((prev) => [...prev, { name: event.name, call: event.summary ?? "" }]);
        } else if (event.kind === "tool_result") {
          setTools((prev) => {
            // 回填最后一个同名未完成调用
            for (let i = prev.length - 1; i >= 0; i--) {
              if (prev[i].name === event.name && prev[i].result === undefined) {
                const next = [...prev];
                next[i] = { ...next[i], result: event.summary ?? "" };
                return next;
              }
            }
            return prev;
          });
        } else {
          setTools((prev) => [...prev, { name: "phase", call: event.name }]);
        }
      },
    );
  });

  onCleanup(() => unlisten?.());

  const send = async () => {
    const text = draft().trim();
    if (!text || streaming()) return;
    setDraft("");
    setStreaming(true);
    setMessages((prev) => [
      ...prev,
      { role: "user", content: text },
      { role: "assistant", content: "" },
    ]);
    const history = messages().map((m) => ({ role: m.role, content: m.content }));
    await sendMessage(text, history);
  };

  return (
    <div class="h-full flex flex-col">
      <div class="px-4 py-2 border-b border-gray-800 text-xs text-gray-500 flex justify-between">
        <span>{model()}</span>
        <Show when={streaming()}>
          <span class="text-indigo-400">streaming…</span>
        </Show>
      </div>
      <div ref={(el) => (listRef = el)} class="flex-1 overflow-auto p-4 space-y-4">
        <For each={messages()}>
          {(m) => (
            <div class={m.role === "user" ? "text-right" : "text-left"}>
              <div
                class={`inline-block max-w-[80%] px-3 py-2 rounded-lg text-sm whitespace-pre-wrap ${
                  m.role === "user" ? "bg-indigo-600 text-white" : "bg-gray-800 text-gray-100"
                }`}
              >
                <Show when={m.reasoning}>
                  <div class="text-xs text-gray-500 border-l-2 border-gray-700 pl-2 mb-1 whitespace-pre-wrap">
                    {m.reasoning}
                  </div>
                </Show>
                {m.content}
                <Show when={m.usage}>
                  <div class="text-xs text-gray-500 mt-1">
                    in {m.usage!.input} / out {m.usage!.output}
                  </div>
                </Show>
                <Show when={m.error}>
                  <div class="text-xs text-rose-400 mt-1">{m.error}</div>
                </Show>
              </div>
            </div>
          )}
        </For>
        <Show when={tools().length > 0}>
          <div class="space-y-1">
            <For each={tools()}>
              {(t) => (
                <div class="text-xs border border-gray-800 rounded px-2 py-1 bg-gray-900/50">
                  <span class="text-indigo-400 font-mono">{t.name}</span>
                  <span class="text-gray-500 ml-2">{t.call}</span>
                  <Show when={t.result}>
                    <div class="text-gray-400 mt-0.5">{t.result}</div>
                  </Show>
                </div>
              )}
            </For>
          </div>
        </Show>
        <Show when={messages().length === 0}>
          <div class="text-gray-600 text-sm text-center mt-20">发一条消息开始</div>
        </Show>
      </div>
      <div class="p-3 border-t border-gray-800 flex gap-2">
        <textarea
          class="flex-1 bg-gray-900 border border-gray-700 rounded px-3 py-2 text-sm resize-none focus:outline-none focus:border-indigo-500"
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
          class="px-4 rounded bg-indigo-600 hover:bg-indigo-500 text-sm disabled:opacity-40"
          onClick={() => void send()}
          disabled={streaming() || !draft().trim()}
        >
          发送
        </button>
      </div>
    </div>
  );
}
