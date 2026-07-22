// 语音输入多引擎：apple（Rust 侧 Speech.framework 本地识别）+ provider（OpenAI 兼容转写）。
// 全部经 WS RPC 驱动；partial 事件走 llm.delta 通道（kind=voice.*）。

import { client } from "./client";

export interface VoiceEngineInfo {
  id: string;
  label: string;
  status: string;
  detail: string;
}

export interface VoiceOverview {
  engine: string;
  fallback: string[];
  locale: string;
  engines: VoiceEngineInfo[];
}

export function voiceEngines(): Promise<VoiceOverview> {
  return client.rpc("voice.engines");
}

export function setVoiceEngine(engine: string, fallback: string[] = []): Promise<void> {
  return client.rpc("voice.set_engine", { engine, fallback });
}

export function setVoiceProviderKey(provider: string, key: string): Promise<void> {
  return client.rpc("voice.set_provider_key", { provider, key });
}

export interface VoiceSession {
  /** 松开 PTT：停止并返回最终文本（apple 等 final；provider 上传转写）。 */
  stop: () => Promise<string | null>;
}

interface VoiceEventPayload {
  kind?: string;
  text?: string;
  message?: string;
}

/** 开始语音会话：partial 实时回调（当前完整假设，非增量）；错误回调。 */
export async function startVoiceSession(
  engine: string | undefined,
  onPartial: (text: string) => void,
  onError: (msg: string) => void,
): Promise<VoiceSession> {
  const off = client.stream("llm.delta").on((payload) => {
    const p = payload as VoiceEventPayload;
    if (p.kind === "voice.partial" && p.text) onPartial(p.text);
    if (p.kind === "voice.error") onError(p.message ?? "语音引擎错误");
  });
  try {
    await client.rpc("voice.start", engine ? { engine } : {});
  } catch (e) {
    off();
    throw e;
  }
  return {
    stop: async () => {
      off();
      const r = await client.rpc<{ text: string | null }>("voice.stop");
      return r.text ?? null;
    },
  };
}
