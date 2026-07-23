// 存储消息 -> 时间线条目（工具调用/推理/文本按序还原）。
import type { RunStats, StoredMessage } from "./chat";

export interface MsgItem {
  kind: "msg";
  role: "user" | "assistant";
  content: string;
  reasoning?: string | undefined;
  stats?: RunStats | undefined;
  error?: string | undefined;
  messageId?: string | undefined;
}
export interface ToolItem {
  kind: "tool";
  name: string;
  call: string;
  result?: string | undefined;
}
export interface PhaseItem {
  kind: "phase";
  name: string;
}
export interface ApprovalItem {
  kind: "approval";
  approvalId: string;
  command: string;
  reason: string;
  resolved?: "allowed" | "denied";
}
export type Item = MsgItem | ToolItem | PhaseItem | ApprovalItem;

export function toItems(messages: StoredMessage[]): Item[] {
  const items: Item[] = [];
  for (const m of messages) {
    if (m.role === "system") continue;
    for (const p of m.parts) {
      if (p.type === "text" && p.text) {
        const last = items.at(-1);
        if (last?.kind === "msg" && last.role === m.role) {
          items[items.length - 1] = {
            ...last,
            content: `${last.content}\n${p.text}`,
            messageId: m.id,
          };
        } else {
          items.push({ kind: "msg", role: m.role, content: p.text, messageId: m.id });
        }
      } else if (p.type === "reasoning" && p.text && m.role === "assistant") {
        const last = items.at(-1);
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
