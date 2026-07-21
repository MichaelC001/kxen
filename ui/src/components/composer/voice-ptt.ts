// 语音 PTT 状态机：长按空格 >=400ms 进语音（撤销误输入空格），松开提交；service-not-allowed 本会话禁用。
import { speechSupported, startVoice, type VoiceSession } from "../../lib/voice";

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
  setDead: (v: boolean) => void;
}): VoiceController {
  let session: VoiceSession | null = null;
  let base = "";
  let finals = "";
  let pttTimer: ReturnType<typeof setTimeout> | undefined;
  let pttActive = false;
  let spaceCountAtDown = 0;

  function start() {
    opts.setError("");
    base = opts.getText();
    finals = "";
    const s = startVoice(
      (interim) => {
        opts.setText(base + finals + interim);
        opts.afterChange();
      },
      (final) => {
        finals += `${final} `;
      },
      (error) => {
        if (error === "service-not-allowed") {
          // webview 语音服务不可用：本会话内禁用，不再反复报错
          opts.setDead(true);
        } else {
          opts.setError(error === "not-allowed" ? "麦克风权限被拒（系统设置 > 隐私 > 麦克风 中允许 kxen）" : `语音识别错误: ${error}`);
        }
        stop();
      },
    );
    if (!s) {
      opts.setDead(true);
      return;
    }
    session = s;
    opts.setRecording(true);
  }

  function stop() {
    session?.stop();
    session = null;
    opts.setRecording(false);
  }

  return {
    toggle: () => {
      if (!speechSupported()) {
        opts.setDead(true);
        return;
      }
      if (session) stop();
      else start();
    },
    stop,
    onSpaceDown: (e) => {
      if (e.key !== " " || e.repeat || pttActive || session) return;
      spaceCountAtDown = opts.getText().length;
      pttTimer = setTimeout(() => {
        pttActive = true;
        // 撤销按下期间误输入的空格再进语音
        if (opts.getText().length > spaceCountAtDown) {
          opts.setText(opts.getText().slice(0, spaceCountAtDown));
        }
        start();
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
        stop();
      }
    },
  };
}
