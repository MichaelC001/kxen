import { createEffect, createSignal, For, Show, onCleanup, onMount } from "solid-js";
import {
  onLlmDelta,
  sendMessage,
  sessionAbort,
  sessionExport,
  sessionFork,
  sessionMessages,
  statusline,
  type ContextItem,
} from "../lib/chat";
import {
  activeSessionId,
  ensureActiveSession,
  refreshSessions,
  sessions,
  setHasConversation,
  switchSession,
} from "../lib/state";
import { onDragStart } from "../lib/drag";
import Markdown from "../components/Markdown";
import ThinkingOrb from "../components/ThinkingOrb";
import type { OrbState } from "../lib/orb";
import ToolCard from "../components/ToolCard";
import Composer from "../components/composer/LexicalComposer";
import { Download, FolderOpen, GitFork, Target, Users, Workflow, Wrench } from "lucide-solid";
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
  const doExport = async () => {
    const r = await sessionExport(activeSessionId()).catch(() => null);
    setExportNote(r ? `已导出 ${r.path}` : "导出失败");
    setTimeout(() => setExportNote(""), 3000);
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
                  <div class="group relative flex justify-end items-start gap-1.5">
                    <Show when={item.messageId}>
                      <button
                        class="opacity-0 group-hover:opacity-100 pressable mt-1 px-1 rounded text-[var(--text-faint)] hover:text-[var(--text)]"
                        title="从此消息分叉"
                        onClick={() => void forkAt(item.messageId!)}
                      >
                        <GitFork size={12} />
                      </button>
                    </Show>
                    <div class="max-w-[80%] rounded-2xl rounded-br-md px-3.5 py-2 text-sm bg-[var(--accent)] text-[var(--accent-contrast)] whitespace-pre-wrap">
                      {item.content}
                    </div>
                  </div>
                );
              }
              // assistant：全宽排版，无气泡（现代 agent UI 形态）
              return (
                <div class="group relative text-sm">
                  <Show when={item.messageId}>
                    <button
                      class="absolute -left-6 top-0.5 opacity-0 group-hover:opacity-100 pressable px-1 rounded text-[var(--text-faint)] hover:text-[var(--text)]"
                      title="从此消息分叉"
                      onClick={() => void forkAt(item.messageId!)}
                    >
                      <GitFork size={12} />
                    </button>
                  </Show>
                  <Show when={item.reasoning}>
                    <div class="text-xs text-[var(--text-faint)] border-l-2 border-[var(--border)] pl-2.5 mb-2 whitespace-pre-wrap">
                      {item.reasoning}
                    </div>
                  </Show>
                  <Markdown text={item.content} />
                  <Show when={item.stats}>
                    {(stats) => (
                      <div class="text-2xs text-[var(--text-faint)] mt-1.5 tabular-nums">
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
            <div class="pt-16 space-y-8 w-full">
              <div class="empty-hero flex items-center gap-4">
                <img
                  src="/icon.png"
                  alt="kxen"
                  class="w-14 h-14 rounded-2xl shadow-lg shadow-indigo-500/20"
                />
                <div>
                  <div class="text-lg font-semibold tracking-tight">kxen</div>
                  <div class="text-xs text-[var(--text-dim)]">
                    多模型并行工作 · 目标驱动 · 团队编排
                  </div>
                </div>
              </div>
              <div class="grid grid-cols-2 gap-2.5">
                {[
                  {
                    icon: Target,
                    title: "write-goal",
                    desc: "定义带完成判据的目标，自动推进直到验证通过",
                  },
                  { icon: Wrench, title: "@ 与 /", desc: "@ 引用文件目录，/ 唤起命令与 skills" },
                  {
                    icon: Workflow,
                    title: "workflow",
                    desc: "我自己写编排脚本，并行派发多个子代理",
                  },
                  {
                    icon: Users,
                    title: "agent teams",
                    desc: "spawn 多模型 teammates 组队干活，各自独立上下文",
                  },
                ].map((c, i) => (
                  <div
                    class="empty-card rounded-xl border border-[var(--border)] bg-[var(--bg-raised)] p-3.5 space-y-1.5"
                    style={`animation-delay: ${80 + i * 50}ms`}
                  >
                    <c.icon size={16} class="text-[var(--accent-hover)]" />
                    <div class="text-xs font-medium font-mono">{c.title}</div>
                    <div class="text-xs leading-snug text-[var(--text-faint)]">{c.desc}</div>
                  </div>
                ))}
              </div>
              <div
                class="empty-card text-xs text-[var(--text-faint)]"
                style="animation-delay: 300ms"
              >
                输入消息开始 · @ 引用 · / 命令 · # 沉淀 · 粘贴图片
              </div>
            </div>
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
