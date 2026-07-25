import type { Setter } from "solid-js";
import { applyApprovalEvent, applyApprovalResolved } from "./approvals";
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
          // output 是完整结果（流式展开区透传）；缺省回退一行摘要
          next[i] = { ...item, result: event.output ?? event.summary ?? "" };
          return next;
        }
      }
      return prev;
    });
  } else if (event.kind === "approval") {
    deps.setOrbPhase("thinking");
    applyApprovalEvent(deps.setItems, event);
  } else if (event.kind === "approval_resolved") {
    // 超时/取消的了结帧：等待中的审批卡置失效
    if (event.approvalId)
      applyApprovalResolved(deps.setItems, event.approvalId, event.outcome ?? "cancelled");
  } else {
    // workflow phase：有 index/total 时 `phase 2/10 · xxx`（避免 phase: + i/N: 双冒号），无则 `phase: xxx`（旧脚本无 meta）
    const label =
      event.index != null && event.total != null
        ? `phase ${event.index}/${event.total} · ${event.name}`
        : `phase: ${event.name}`;
    deps.setItems((prev) => [...prev, { kind: "phase", name: label }]);
  }
  deps.scroll();
}
