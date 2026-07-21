import { rpc } from "./rpc";
import { subscribe } from "./stream";
import type { DoctorReport } from "./tauri";

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
  reasoning?: string;
  usage?: { input: number; output: number };
  error?: string;
}

export type { DoctorReport };

export async function doctor(): Promise<DoctorReport> {
  return rpc<DoctorReport>("doctor");
}

export async function currentModel(): Promise<{ provider: string; model: string }> {
  return rpc("current_model");
}

export async function sendMessage(
  text: string,
  history: Array<{ role: string; content: string }>,
): Promise<void> {
  return rpc("send_message", { text, history });
}

export interface ToolEvent {
  kind: "tool_call" | "tool_result" | "phase";
  name: string;
  summary?: string;
}

export function onLlmDelta(
  onText: (text: string) => void,
  onReasoning: (text: string) => void,
  onDone: (usage?: { input: number; output: number }, error?: string) => void,
  onTool?: (event: ToolEvent) => void,
): Promise<() => void> {
  let usage: { input: number; output: number } | undefined;
  return subscribe(["llm.delta"], (_topic, payload) => {
    handle(
      payload as {
        kind?: string;
        text?: string;
        input?: number;
        output?: number;
        message?: string;
        name?: string;
        summary?: string;
      },
    );
  });

  function handle(event: {
    kind?: string;
    text?: string;
    input?: number;
    output?: number;
    message?: string;
    name?: string;
    summary?: string;
  }) {
    switch (event.kind) {
      case "text":
        if (event.text) onText(event.text);
        break;
      case "reasoning":
        if (event.text) onReasoning(event.text);
        break;
      case "usage":
        usage = { input: event.input ?? 0, output: event.output ?? 0 };
        break;
      case "done":
        onDone(usage);
        break;
      case "error":
        onDone(undefined, event.message ?? "unknown error");
        break;
      case "tool_call":
      case "tool_result":
      case "phase":
        if (event.name) onTool?.({ kind: event.kind, name: event.name, summary: event.summary });
        break;
    }
  }
}
