// 语音 PTT 状态机：长按空格 >=400ms 进语音（撤销误输入空格），松开提交；引擎由 MicMenu/设置页选择。
import { startVoiceSession, type VoiceSession } from "../../lib/voice";

export interface VoiceController {
  toggle: () => void;
  stop: () => void;
  onSpaceDown: (e: KeyboardEvent) => void;
  onSpaceUp: (e: KeyboardEvent) => void;
}

export function createVoicePtt(opts: {
  getText: () => string;
  setText: (v: string) => void;
  afterChange: () => void;
  setRecording: (v: boolean) => void;
  setError: (v: string) => void;
  engine: () => string;
}): VoiceController {
  let session: VoiceSession | null = null;
  let starting = false;
  let cancelled = false;
  let base = "";
  let pttTimer: ReturnType<typeof setTimeout> | undefined;
  let pttActive = false;
  let spaceCountAtDown = 0;

  async function start() {
    if (session || starting) return;
    starting = true;
    cancelled = false;
    opts.setError("");
    base = opts.getText();
    try {
      const s = await startVoiceSession(
        opts.engine(),
        (partial) => {
          opts.setText(base + partial);
          opts.afterChange();
        },
        (msg) => {
          opts.setError(msg);
          void stop();
        },
      );
      if (cancelled) {
        void s.stop();
        return;
      }
      session = s;
      opts.setRecording(true);
    } catch (e) {
      opts.setError(e instanceof Error ? e.message : String(e));
    } finally {
      starting = false;
    }
  }

  async function stop() {
    const s = session;
    session = null;
    cancelled = true;
    opts.setRecording(false);
    if (!s) return;
    const finalText = await s.stop().catch(() => null);
    if (finalText) {
      opts.setText(base + finalText);
      opts.afterChange();
    }
  }

  return {
    toggle: () => {
      if (session) void stop();
      else void start();
    },
    stop: () => void stop(),
    onSpaceDown: (e) => {
      if (e.key !== " " || e.repeat || pttActive || session || starting) return;
      spaceCountAtDown = opts.getText().length;
      pttTimer = setTimeout(() => {
        pttActive = true;
        // 撤销按下期间误输入的空格再进语音
        if (opts.getText().length > spaceCountAtDown) {
          opts.setText(opts.getText().slice(0, spaceCountAtDown));
        }
        void start();
      }, 400);
    },
    onSpaceUp: (e) => {
      if (e.key !== " ") return;
      if (pttTimer) {
        clearTimeout(pttTimer);
        pttTimer = undefined;
      }
      if (pttActive) {
        pttActive = false;
        void stop();
      }
    },
  };
}
