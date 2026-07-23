import { createEffect, createSignal, For, Show, onCleanup, onMount } from "solid-js";
import {
  currentModel,
  onLlmDelta,
  sendMessage,
  sessionAbort,
  sessionExport,
  sessionMessages,
  sessionPendingList,
  sessionRewind,
  statusline,
  type ContextItem,
} from "../lib/chat";
import { createConverge } from "../lib/converge";
import { createDeltaBatcher } from "../lib/delta-batch";
import { applyApprovalEvent, respondApproval as respondApprovalImpl } from "../lib/approvals";
import { editResend as editResendImpl, forkAt, rerun as rerunImpl } from "../lib/session-actions";
import { displayName, modelsCatalog } from "../lib/models";
import AssistantItem from "../components/AssistantItem";
import ApprovalCard from "../components/ApprovalCard";
import PendingQueue from "../components/PendingQueue";
import UserItem from "../components/UserItem";
import {
  activeSessionId,
  ensureActiveSession,
  newSession,
  sessions,
  setHasConversation,
} from "../lib/state";
import { onDragStart } from "../lib/drag";
import ThinkingOrb from "../components/ThinkingOrb";
import type { OrbState } from "../lib/orb";
import EmptyHero from "../components/EmptyHero";
import ToolCard from "../components/ToolCard";
import Composer from "../components/composer/TextComposer";
import { ArrowDown, Download, FolderOpen } from "lucide-solid";
import { toItems, type Item } from "../lib/items";

export default function Session() {
  const [items, setItems] = createSignal<Item[]>([]);
  const [streamingSid, setStreamingSid] = createSignal("");
  const [orbPhase, setOrbPhase] = createSignal<OrbState>("thinking");
  const [focusTick, setFocusTick] = createSignal(0);
  const [workdir, setWorkdir] = createSignal("");
  let unlisten: (() => void) | undefined;
  let listRef: HTMLDivElement | undefined;
  let prevSid = "";
  const [pendingQueue, setPendingQueue] = createSignal<string[]>([]);

  const streaming = () => streamingSid() === activeSessionId() && activeSessionId() !== "";
  const title = () =>
    activeSessionId() === ""
      ? "新会话"
      : (sessions().find((s) => s.id === activeSessionId())?.title ?? "会话");
  // 钉底跟随：用户上翻即停跟（每 delta 硬拉到底 = 滚动闪烁的根因），底部给回跳按钮
  const [pinned, setPinned] = createSignal(true);
  const onListScroll = () =>
    listRef && setPinned(listRef.scrollHeight - listRef.scrollTop - listRef.clientHeight < 48);
  const scroll = (force = false) => {
    if (force || pinned()) {
      // rAF 等布局完成再钉底（queueMicrotask 抢在 layout 前，位置算错再纠偏 = 闪）
      requestAnimationFrame(() => {
        if (listRef) listRef.scrollTop = listRef.scrollHeight;
        setPinned(true);
      });
    }
  };

  // 有对话内容才驱动右 dock 滑入
  createEffect(() => setHasConversation(items().length > 0));

  // 切换会话：加载存储的时间线；草稿态（""）清空。
  // 草稿->激活（首发）跳过重载：此时本地上屏是唯一权威（空载会抹掉乐观消息 = 首行消失的根因）。
  createEffect(() => {
    const id = activeSessionId();
    setFocusTick((t) => t + 1);
    if (!id) {
      setItems([]);
      setPendingQueue([]);
      prevSid = id;
      return;
    }
    if (prevSid !== id) {
      void sessionPendingList(id).then((q) => {
        if (activeSessionId() === id) setPendingQueue(q);
      });
    }
    const fromDraft = prevSid === "";
    prevSid = id;
    if (fromDraft) return;
    void sessionMessages(id).then((messages) => {
      if (activeSessionId() === id) {
        setItems(toItems(messages));
        scroll();
      }
    });
  });

  /** Done 对账（实现见 lib/converge.ts）：快照权威 + 队列真源。 */
  const { converge, clearQueue } = createConverge({
    setItems,
    setPendingQueue,
    scroll: () => scroll(),
  });

  const appendRaw = (field: "content" | "reasoning", text: string) => {
    setItems((prev) => {
      const last = prev.at(-1);
      if (last?.kind === "msg" && last.role === "assistant") {
        return [...prev.slice(0, -1), { ...last, [field]: (last[field] ?? "") + text }];
      }
      const msg = {
        kind: "msg" as const,
        role: "assistant" as const,
        content: field === "content" ? text : "",
        reasoning: field === "reasoning" ? text : undefined,
      };
      return [...prev, msg];
    });
    scroll();
  };

  // delta 批量上屏：50ms 合并（实现见 lib/delta-batch.ts）
  const batcher = createDeltaBatcher(appendRaw);
  const appendAssistant = (field: "content" | "reasoning", text: string) => {
    setOrbPhase("composing");
    batcher.push(field, text);
  };

  onMount(async () => {
    const sl = await statusline("").catch(() => null);
    if (sl) setWorkdir(sl.workdir);
    const m = await currentModel().catch(() => null);
    if (m) setModelLabel(displayName(await modelsCatalog().catch(() => []), m.provider, m.model));
    unlisten = await onLlmDelta(
      activeSessionId,
      (text) => appendAssistant("content", text),
      (reasoning) => appendAssistant("reasoning", reasoning),
      (stats, error) => {
        setOrbPhase(error ? "error" : "thinking");
        setStreamingSid("");
        batcher.flushNow(); // 残余 delta 先上屏再对账 // 残余 delta 先上屏再对账
        // Done 对账：存储快照为最终权威（含终态文本），stats/error 尾注重挂
        converge(activeSessionId(), { stats, error });
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
        } else if (event.kind === "approval") {
          setOrbPhase("thinking");
          applyApprovalEvent(setItems, event);
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
    // 不在前端拦截并发：后端按会话排队（此处静默 return 曾经直接吞掉用户消息）
    const sid = await ensureActiveSession();
    if (!streaming()) {
      setStreamingSid(sid);
      setOrbPhase("thinking");
    }
    setItems((prev) => [...prev, { kind: "msg", role: "user", content: text }]);
    scroll(true); // 自己发的消息强制到底
    const r = await sendMessage(sid, text, context, images).catch(() => null);
    if (r?.queued) setPendingQueue((prev) => [...prev, text]);
  };

  const stop = () => {
    const sid = activeSessionId();
    if (sid) {
      setPendingQueue([]);
      void sessionAbort(sid);
    }
  };

  const respondApproval = (id: string, allow: boolean) => respondApprovalImpl(setItems, id, allow);

  const [exportNote, setExportNote] = createSignal("");
  const [modelLabel, setModelLabel] = createSignal("");
  const doExport = async () => {
    const r = await sessionExport(activeSessionId()).catch(() => null);
    setExportNote(r ? `已导出 ${r.path}` : "导出失败");
    setTimeout(() => setExportNote(""), 3000);
  };

  const rerun = (idx: number) => rerunImpl(send, items(), idx);

  /** 回退到指定消息：shadow git 代码回滚 + 会话截断，随后真源重载。 */
  const rewindAt = async (messageId: string) => {
    const sid = activeSessionId();
    if (!sid) return;
    await sessionRewind(sid, messageId).catch(() => {});
    converge(sid);
  };
  const editResend = async (idx: number, text: string) => {
    const done = await editResendImpl(send, items(), idx, text);
    if (!done) {
      await newSession();
      await send(text, [], []);
    }
  };

  return (
    <div class="h-full flex-1 min-w-0 flex flex-col relative">
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

      <div
        ref={(el) => (listRef = el)}
        class="flex-1 overflow-auto px-4 py-5"
        onScroll={onListScroll}
      >
        <div class="w-full space-y-4">
          <For each={items()}>
            {(item, i) => {
              if (item.kind === "tool") {
                return <ToolCard name={item.name} call={item.call} result={item.result} />;
              }
              if (item.kind === "approval") {
                return (
                  <ApprovalCard
                    item={item}
                    onRespond={(id, allow) => void respondApproval(id, allow)}
                  />
                );
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
                  <UserItem
                    item={item}
                    onFork={() => void forkAt(item.messageId!)}
                    onEditResend={(text) => void editResend(i(), text)}
                    onRewind={() => void rewindAt(item.messageId!)}
                  />
                );
              }
              // assistant：全宽排版，无气泡（现代 agent UI 形态）
              return (
                <AssistantItem
                  item={item}
                  streaming={streaming}
                  live={() => streaming() && i() === items().length - 1}
                  modelLabel={modelLabel}
                  onFork={() => void forkAt(item.messageId!)}
                  onRerun={() => void rerun(i())}
                  onContinue={() => void send("继续", [], [])}
                  onRewind={() => void rewindAt(item.messageId!)}
                />
              );
            }}
          </For>

          <Show when={items().length === 0}>
            <EmptyHero />
          </Show>
        </div>
      </div>

      <Show when={!pinned()}>
        <button
          class="pressable absolute left-1/2 -translate-x-1/2 bottom-24 z-20 px-2.5 py-1 rounded-full text-2xs border border-[var(--border)] bg-[var(--bg-raised)] text-[var(--text-dim)] composer-popup flex items-center gap-1"
          onClick={() => scroll(true)}
        >
          <ArrowDown size={11} /> 回到底部
        </button>
      </Show>

      <div class="px-3 pb-3 composer-fade">
        <div class="w-full">
          <PendingQueue queue={pendingQueue} onClear={() => void clearQueue()} />
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
