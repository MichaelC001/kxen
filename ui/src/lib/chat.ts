import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
  reasoning?: string;
  usage?: { input: number; output: number };
  error?: string;
}

interface SendInput {
  text: string;
  history: Array<{ role: string; content: string }>;
}

type LlmEvent =
  | { kind: "text"; text: string }
  | { kind: "reasoning"; text: string }
  | { kind: "usage"; input: number; output: number }
  | { kind: "done" }
  | { kind: "error"; message: string };

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export async function sendMessage(input: SendInput): Promise<void> {
  if (!isTauri) return;
  return tauriInvoke("send_message", { input });
}

export async function currentModel(): Promise<{ provider: string; model: string }> {
  if (!isTauri) return { provider: "xai", model: "grok-build-0.1 (browser mock)" };
  return tauriInvoke("current_model");
}

export async function onLlmDelta(
  onText: (text: string) => void,
  onReasoning: (text: string) => void,
  onDone: (usage?: { input: number; output: number }, error?: string) => void,
): Promise<UnlistenFn> {
  if (!isTauri) return () => {};
  let usage: { input: number; output: number } | undefined;
  return listen<LlmEvent>("llm://delta", (event) => {
    const payload = event.payload;
    switch (payload.kind) {
      case "text":
        onText(payload.text);
        break;
      case "reasoning":
        onReasoning(payload.text);
        break;
      case "usage":
        usage = { input: payload.input, output: payload.output };
        break;
      case "done":
        onDone(usage);
        break;
      case "error":
        onDone(undefined, payload.message);
        break;
    }
  });
}
