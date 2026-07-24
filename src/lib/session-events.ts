import type { Setter } from "solid-js";
import { applyApprovalEvent } from "./approvals";
import type { ToolEvent } from "./delta";
import type { Item } from "./items";
import type { OrbState } from "./orb";

// 流式 delta 合并到尾部 assistant 气泡的纯 reducer（Session 页与测试共用）
export function appendRawItem(prev: Item[], field: "content" | "reasoning", text: string): Item[] {
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
}

// tool/approval/phase 事件统一上屏；从 Session.tsx 拆出（350 行门禁收口），行为与原闭包一致
export function applyStreamEvent(
  event: ToolEvent,
  deps: { setItems: Setter<Item[]>; setOrbPhase: Setter<OrbState>; scroll: () => void },
): void {
  if (event.kind === "tool_call") {
    deps.setOrbPhase("searching");
    deps.setItems((prev) => [
      ...prev,
      { kind: "tool", name: event.name, call: event.summary ?? "", args: event.args },
    ]);
  } else if (event.kind === "tool_result") {
    deps.setItems((prev) => {
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
    deps.setOrbPhase("thinking");
    applyApprovalEvent(deps.setItems, event);
  } else {
    deps.setItems((prev) => [...prev, { kind: "phase", name: event.name }]);
  }
  deps.scroll();
}
