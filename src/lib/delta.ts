// llm.delta 事件流订阅与分发（从 chat.ts 拆出，时间线增量唯一入口）。
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
  return client.stream("llm.delta").on((payload) => {
    handle(payload as DeltaPayload);
  });

  interface DeltaPayload {
    kind?: string;
    session_id?: string;
    text?: string;
    message?: string;
    name?: string;
    summary?: string;
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
