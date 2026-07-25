import { createEffect, createSignal, For, Show, onCleanup, onMount } from "solid-js";
import {
  onLlmDelta,
  sessionAbort,
  sessionExport,
  sessionMessages,
  sessionPendingList,
  sessionRunning,
  statusline,
} from "../lib/chat";
import { createConverge } from "../lib/converge";
import { createDeltaBatcher } from "../lib/delta-batch";
import { respondApproval as respondApprovalImpl } from "../lib/approvals";
import { applyStreamEvent, appendRawItem } from "../lib/session-events";
import { editResend as editResendImpl, forkAt, rerun as rerunImpl } from "../lib/session-actions";
import { createSendFlow } from "../lib/send";
import { createSessionRewind } from "../lib/rewind";
import { createSessionModelLabel } from "../lib/session-model";
import AssistantItem from "../components/AssistantItem";
import ApprovalCard from "../components/ApprovalCard";
import PendingQueue from "../components/PendingQueue";
import RewindConfirm from "../components/RewindConfirm";
import UserItem from "../components/UserItem";
import { activeSessionId, newSession, sessions, setHasConversation } from "../lib/state";
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
  // null 哨兵 = 组件首跑（含路由卸载重挂载）：强制重载时间线；
  // 仅 ""（草稿->激活首发）才跳过重载保住乐观上屏，否则重挂载 items 永远空（时间线空白的根因）
  let prevSid: string | null = null;
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
    setItems((prev) => appendRawItem(prev, field, text));
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
    unlisten = await onLlmDelta(
      activeSessionId,
      (text) => appendAssistant("content", text),
      (reasoning) => appendAssistant("reasoning", reasoning),
      (stats, error) => {
        setOrbPhase(error ? "error" : "thinking");
        setStreamingSid("");
        batcher.flushNow(); // 残余 delta 先上屏再对账
        // Done 对账：存储快照为最终权威（含终态文本），stats/error 尾注重挂
        converge(activeSessionId(), { stats, error });
      },
      (event) => applyStreamEvent(event, { setItems, setOrbPhase, scroll }),
      () => {
        // resync（bus lag / 断线重连）：只对账，streaming 由运行真源决定——
        // run 还在跑：保留 streaming（后续 delta 自然续上，stop 按钮不丢）；
        // done 在断线窗口丢失：running=false 按真源收回；核对失败（null）保守保留等下轮 resync
        batcher.flushNow();
        const sid = activeSessionId();
        if (!sid) return;
        converge(sid);
        if (streamingSid() !== sid) return;
        void sessionRunning(sid).then((running) => {
          if (running === false && streamingSid() === sid) setStreamingSid("");
        });
      },
    );
  });

  onCleanup(() => unlisten?.());

  // 发送链路实现见 lib/send.ts（乐观上屏 + 失败态标记/点击重发）
  const { send, retry: retrySend } = createSendFlow({
    streaming,
    onStreamStart: (sid) => {
      setStreamingSid(sid);
      setOrbPhase("thinking");
    },
    onStreamStop: (sid) => {
      if (streamingSid() === sid) setStreamingSid("");
    },
    setItems,
    setPendingQueue,
    scroll,
  });
  const stop = () => {
    const sid = activeSessionId();
    if (sid) {
      setPendingQueue([]);
      void sessionAbort(sid);
    }
  };

  const respondApproval = (id: string, allow: boolean) => respondApprovalImpl(setItems, id, allow);

  const [exportNote, setExportNote] = createSignal("");
  // assistant 消息署名：当前 session 的生效模型（覆盖优先；切会话/切模型自动重取）
  const modelLabel = createSessionModelLabel(activeSessionId);
  const doExport = async () => {
    const r = await sessionExport(activeSessionId()).catch(() => null);
    setExportNote(r ? `已导出 ${r.path}` : "导出失败");
    setTimeout(() => setExportNote(""), 3000);
  };

  const rerun = (idx: number) => rerunImpl(send, items(), idx);

  const rewind = createSessionRewind({
    sessionId: activeSessionId,
    onDone: () => converge(activeSessionId()),
  });
  const rewindAt = (messageId: string) => void rewind.flow.request(messageId);
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
          <Show when={rewind.note()}>
            <button
              class="pressable text-2xs text-[var(--err)]"
              title="点击关闭"
              onClick={() => rewind.dismissNote()}
            >
              {rewind.note()}
            </button>
          </Show>
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
                return (
                  <ToolCard
                    name={item.name}
                    call={item.call}
                    args={item.args}
                    result={item.result}
                  />
                );
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
                    {item.name}
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
                    onRetry={() => void retrySend(item)}
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
          <Show when={rewind.pending()}>
            <RewindConfirm
              onConfirm={() => void rewind.flow.confirm()}
              onCancel={() => rewind.flow.cancel()}
            />
          </Show>
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
