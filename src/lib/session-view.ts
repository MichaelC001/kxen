// 会话页 Chat / Trajectory 双视图状态 + Chat 工具行 Inspect 联动请求。
// 模块级信号（与 tool-ui expandAllTools 同先例）：Session 页单实例。
// 切走 Chat 记滚动锚点、切回恢复，阅读位置不丢。
import { createSignal } from "solid-js";
import type { ToolItem } from "./items";

export type SessionView = "chat" | "trajectory";

/** Chat 工具行「Inspect」定位目标：落盘消息 id + part 下标。 */
export interface InspectTarget {
  messageId: string;
  partIndex: number;
}

const [view, setView] = createSignal<SessionView>("chat");
const [inspectTarget, setInspectTarget] = createSignal<InspectTarget | null>(null);
let chatList: HTMLDivElement | undefined;
let savedChatScroll = 0;

export const sessionView = view;
export { inspectTarget };

/** Session 页注册 Chat 列表元素：滚动锚点存取都经它。 */
export function registerChatList(element: HTMLDivElement | undefined) {
  chatList = element;
}

export function switchSessionView(next: SessionView) {
  if (view() === next) return;
  if (next === "trajectory") savedChatScroll = chatList?.scrollTop ?? 0;
  setView(next);
  if (next === "chat") {
    queueMicrotask(() => {
      if (chatList) chatList.scrollTop = savedChatScroll;
    });
  }
}

/** Chat 工具行 Inspect：切到 Trajectory 并定位对应记录；无落盘定位的流式条目忽略。 */
export function requestInspectTool(item: ToolItem) {
  if (item.messageId === undefined || item.partIndex === undefined) return;
  setInspectTarget({ messageId: item.messageId, partIndex: item.partIndex });
  switchSessionView("trajectory");
}

export function consumeInspectTarget() {
  setInspectTarget(null);
}
