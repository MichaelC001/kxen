// llm.delta 事件流订阅与分发（从 chat.ts 拆出，时间线增量唯一入口）。
import { createEffect, onCleanup } from "solid-js";
import { client } from "./client";

export interface RunStats {
  ttft_ms: number;
  duration_ms: number;
  input_tokens: number;
  output_tokens: number;
  tokens_per_sec: number;
}

export interface ToolEvent {
  kind: "tool_call" | "tool_result" | "phase" | "approval";
  name: string;
  summary?: string | undefined;
  args?: string | undefined;
  approvalId?: string | undefined;
  command?: string | undefined;
  reason?: string | undefined;
}

export function onLlmDelta(
  activeSession: () => string,
  onText: (text: string) => void,
  onReasoning: (text: string) => void,
  onDone: (stats?: RunStats, error?: string) => void,
  onTool?: (event: ToolEvent) => void,
): () => void {
  let off: (() => void) | undefined;
  let current: string | undefined;
  // bus lag 丢帧后服务端下发 resync：本地时间线已有缺口（done 丢失会卡死 streaming 态），
  // 复用 done 对账通道（flush + 快照重载 + 队列真源）收口；run 仍在跑时后续 delta 自然续上
  const offResync = client.onResync(() => onDone());
  // 后端 stream ACL：带 session_id 的帧只发给订阅了 session:<id> topic 的连接，
  // 订阅必须跟随活跃会话（旧订阅退掉，否则切走后仍占着别会话的帧通道）
  createEffect(() => {
    const sid = activeSession();
    if (sid === current) return;
    current = sid;
    off?.();
    off = client.stream(sid ? ["llm.delta", `session:${sid}`] : ["llm.delta"]).on((payload) => {
      handle(payload as DeltaPayload);
    });
  });
  onCleanup(() => {
    off?.();
    offResync();
  });
  return () => {
    off?.();
    offResync();
  };

  interface DeltaPayload {
    kind?: string;
    session_id?: string;
    text?: string;
    message?: string;
    name?: string;
    summary?: string;
    arguments?: string;
    stats?: RunStats;
    agent?: string;
    approval_id?: string;
    command?: string;
  }

  function handle(event: DeltaPayload) {
    // 只渲染活跃会话的增量（后台运行的其他会话事件忽略）
    if (event.session_id && event.session_id !== activeSession()) return;
    switch (event.kind) {
      case "text":
        if (event.text) onText(event.text);
        break;
      case "reasoning":
        if (event.text) onReasoning(event.text);
        break;
      case "done":
        onDone(event.stats);
        break;
      case "aborted":
        onDone(undefined, "(已中断)");
        break;
      case "error":
        onDone(undefined, event.message ?? "unknown error");
        break;
      case "tool_call":
        if (event.name)
          onTool?.({
            kind: event.kind,
            name: event.name,
            summary: event.summary,
            args: event.arguments,
          });
        break;
      case "tool_result":
      case "phase":
        if (event.name) onTool?.({ kind: event.kind, name: event.name, summary: event.summary });
        break;
      case "approval":
        onTool?.({
          kind: "approval",
          name: "approval",
          approvalId: event.approval_id,
          command: event.command,
          reason: event.message,
        });
        break;
    }
  }
}
