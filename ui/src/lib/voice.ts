// 语音输入：优先 Web Speech API（WKWebView 调 Apple 本地识别，零成本离线可用）。
// 不支持时按钮禁用（提示降级方案），不静默失败。

interface SpeechRecognitionResultItem {
  transcript: string;
  confidence: number;
}
interface SpeechRecognitionResultList {
  [index: number]: SpeechRecognitionResultItem[];
  length: number;
}
interface SpeechRecognitionEventLike {
  resultIndex: number;
  results: SpeechRecognitionResultList & {
    [index: number]: { isFinal: boolean } & SpeechRecognitionResultList[number][];
  };
}
interface SpeechRecognitionLike {
  lang: string;
  continuous: boolean;
  interimResults: boolean;
  onresult: ((e: SpeechRecognitionEventLike) => void) | null;
  onerror: ((e: { error: string }) => void) | null;
  onend: (() => void) | null;
  start: () => void;
  stop: () => void;
  abort: () => void;
}

type SpeechRecognitionCtor = new () => SpeechRecognitionLike;

function ctor(): SpeechRecognitionCtor | null {
  const w = window as unknown as {
    SpeechRecognition?: SpeechRecognitionCtor;
    webkitSpeechRecognition?: SpeechRecognitionCtor;
  };
  return w.SpeechRecognition ?? w.webkitSpeechRecognition ?? null;
}

export function speechSupported(): boolean {
  return ctor() !== null;
}

export interface VoiceSession {
  stop: () => void;
}

/** 开始识别：interim 结果实时回调，final 结果标记。返回停止句柄。 */
export function startVoice(
  onInterim: (text: string) => void,
  onFinal: (text: string) => void,
  onError: (error: string) => void,
): VoiceSession | null {
  const Ctor = ctor();
  if (!Ctor) return null;
  const recognition = new Ctor();
  recognition.lang = "zh-CN";
  recognition.continuous = true;
  recognition.interimResults = true;

  recognition.onresult = (e) => {
    let interim = "";
    for (let i = e.resultIndex; i < e.results.length; i++) {
      const result = e.results[i];
      const transcript = result[0]?.transcript ?? "";
      if ((result as unknown as { isFinal?: boolean }).isFinal) {
        onFinal(transcript);
      } else {
        interim += transcript;
      }
    }
    if (interim) onInterim(interim);
  };
  recognition.onerror = (e) => onError(e.error);
  recognition.onend = null;

  recognition.start();
  return {
    stop: () => recognition.stop(),
  };
}
