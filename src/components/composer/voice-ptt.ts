// 语音 PTT 状态机：长按空格 >=400ms 进语音（激活期/激活后空格一律 preventDefault 防连打），
// 松开提交；startSession 可注入（测试替身），默认走 RPC 引擎。
import { startVoiceSession, type VoiceSession } from "../../lib/voice";

export interface VoiceController {
  toggle: () => void;
  stop: () => void;
  onSpaceDown: (e: KeyboardEvent) => void;
  onSpaceUp: (e: KeyboardEvent) => void;
}

type StartSession = (
  engine: string | undefined,
  onPartial: (text: string) => void,
  onError: (msg: string) => void,
) => Promise<VoiceSession>;

export function createVoicePtt(opts: {
  getText: () => string;
  setText: (v: string) => void;
  afterChange: () => void;
  setRecording: (v: boolean) => void;
  setError: (v: string) => void;
  engine: () => string;
  startSession?: StartSession;
  /** 启动成功回调：回传实际引擎（降级链可能落到非主引擎）。 */
  onStarted?: (engine: string) => void;
}): VoiceController {
  const startSession: StartSession = opts.startSession ?? startVoiceSession;
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
      const s = await startSession(
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
      opts.onStarted?.(s.engine);
    } catch (e) {
      opts.setError(e instanceof Error ? e.message : String(e));
      // 失败复位：PTT 不留激活态（继续按住只剩普通空格键，keyup 自然结束）
      pttActive = false;
    } finally {
      starting = false;
    }
  }

  async function stop() {
    const s = session;
    session = null;
    cancelled = true;
    pttActive = false;
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
      if (e.key !== " ") return;
      // PTT 已激活或启动中：空格一律不入字（防连打）
      if (pttActive || session || starting) {
        e.preventDefault();
        return;
      }
      if (e.repeat) {
        // 激活期（0-400ms）内的自动重复同样不入字
        if (pttTimer) e.preventDefault();
        return;
      }
      spaceCountAtDown = opts.getText().length;
      pttTimer = setTimeout(() => {
        pttActive = true;
        // 撤销激活期误输入的空格再进语音
        if (opts.getText().length > spaceCountAtDown) {
          opts.setText(opts.getText().slice(0, spaceCountAtDown));
          opts.afterChange();
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
