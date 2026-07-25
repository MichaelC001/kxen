// 回退编排：dirty 门禁的确认闭环 + 拒绝文案人话化（供 Session 页与单测共用）。
// 错误串子串与 src-tauri/src/ws/session_ops.rs 的 RewindBlock 文案一一对应。
import { createSignal } from "solid-js";
import { sessionRewind } from "./chat";

/** 后端 rewind 门禁拒绝类别。 */
export type RewindBlock = "active_run" | "not_in_session" | "dirty" | "unknown";

export function classifyRewindError(err: unknown): RewindBlock {
  const msg = err instanceof Error ? err.message : String(err);
  if (msg.includes("active run")) return "active_run";
  if (msg.includes("message not found")) return "not_in_session";
  if (msg.includes("uncheckpointed changes")) return "dirty";
  return "unknown";
}

/** 三种拒绝各一句人话；未识别错误带上原始信息便于排查。 */
export function rewindErrorText(err: unknown): string {
  const raw = err instanceof Error ? err.message : String(err);
  switch (classifyRewindError(err)) {
    case "active_run":
      return "工作区有任务正在运行，回退会覆盖它正在写的文件，请先停止或等它完成";
    case "not_in_session":
      return "这条消息不在当前会话中，无法回退到此处";
    case "dirty":
      return "工作区有未进检查点的改动";
    default:
      return raw ? `回退失败：${raw}` : "回退失败";
  }
}

export interface RewindFlow {
  /** 等待 dirty 确认的 messageId，无待确认项为 null。 */
  pending: () => string | null;
  /** 发起回退：dirty 且无 confirm 转待确认态；active_run / not_in_session 直接报错，不重试。 */
  request: (messageId: string) => Promise<void>;
  /** 用户确认覆盖未进检查点的改动：带 confirm=true 重发。 */
  confirm: () => Promise<void>;
  /** 放弃待确认的回退。 */
  cancel: () => void;
}

/** 确认流与 UI 解耦：页面注入 sid 获取与回调，测试注入 call 断言调用序列。 */
export function createRewindFlow(deps: {
  sessionId: () => string;
  call?: (sessionId: string, messageId: string, confirm: boolean) => Promise<unknown>;
  onPendingChange?: (messageId: string | null) => void;
  onDone?: () => void;
  onError?: (text: string) => void;
}): RewindFlow {
  const call = deps.call ?? sessionRewind;
  let pendingId: string | null = null;
  const setPending = (id: string | null) => {
    pendingId = id;
    deps.onPendingChange?.(id);
  };

  const run = async (messageId: string, confirm: boolean): Promise<void> => {
    const sid = deps.sessionId();
    if (!sid) return;
    try {
      await call(sid, messageId, confirm);
      setPending(null);
      deps.onDone?.();
    } catch (err) {
      if (classifyRewindError(err) === "dirty" && !confirm) {
        setPending(messageId);
        return;
      }
      setPending(null);
      deps.onError?.(rewindErrorText(err));
    }
  };

  return {
    pending: () => pendingId,
    request: (messageId) => run(messageId, false),
    confirm: async () => {
      const id = pendingId;
      if (id) await run(id, true);
    },
    cancel: () => setPending(null),
  };
}

/** Session 页接线（350 门禁拆出）：信号 + 确认流 + 错误尾注一次给齐。 */
export function createSessionRewind(deps: {
  sessionId: () => string;
  onDone: () => void;
  call?: (sessionId: string, messageId: string, confirm: boolean) => Promise<unknown>;
}) {
  const [pending, setPending] = createSignal<string | null>(null);
  const [note, setNote] = createSignal("");
  let timer: ReturnType<typeof setTimeout> | undefined;
  const showNote = (text: string) => {
    // 连续报错只留一个计时器：旧计时器不抢清新文案
    if (timer) clearTimeout(timer);
    setNote(text);
    timer = setTimeout(() => setNote(""), 4000);
  };
  const dismissNote = () => {
    if (timer) clearTimeout(timer);
    setNote("");
  };
  // 成功才对账：失败的回退不动时间线，不触发无意义重载
  const flow = createRewindFlow({
    sessionId: deps.sessionId,
    // exactOptionalPropertyTypes：显式 undefined 不能传给可选属性，有注入才带上
    ...(deps.call ? { call: deps.call } : {}),
    onPendingChange: setPending,
    onDone: deps.onDone,
    onError: showNote,
  });
  return { pending, note, flow, dismissNote };
}
