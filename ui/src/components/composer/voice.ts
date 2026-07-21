// 语音输入状态机：权限申请 -> 识别 -> interim 更新语音节点；PTT 长按空格（>=400ms）。
import type { LexicalEditor, NodeKey } from "lexical";
import { ensureMicPermission, startVoice, type VoiceSession } from "../../lib/voice";
import { upsertVoiceText } from "./editor";

export interface VoiceController {
  recording: () => boolean;
  error: () => string;
  start: () => void;
  stop: () => void;
  onSpaceDown: (e: KeyboardEvent) => void;
  onSpaceUp: (e: KeyboardEvent) => void;
}

export function createVoiceController(
  editor: () => LexicalEditor | null,
  setRecording: (v: boolean) => void,
  setError: (v: string) => void,
): VoiceController {
  let session: VoiceSession | null = null;
  let nodeKey: NodeKey | null = null;
  let finals = "";
  let pttTimer: ReturnType<typeof setTimeout> | undefined;
  let pttActive = false;

  function start() {
    setError("");
    void (async () => {
      // 先显式申请（首次触发系统授权弹窗），通过后才启动识别
      const perm = await ensureMicPermission();
      if (!perm.ok) {
        setError(perm.error ?? "麦克风不可用");
        return;
      }
      finals = "";
      nodeKey = null;
      const s = startVoice(
        (interim) => {
          const ed = editor();
          if (ed) nodeKey = upsertVoiceText(ed, nodeKey, finals + interim);
        },
        (final) => {
          finals += `${final} `;
        },
        (error) => {
          setError(
            error === "not-allowed"
              ? "麦克风权限被拒（系统设置 > 隐私 > 麦克风 中允许 kxen）"
              : `语音识别错误: ${error}`,
          );
          stop();
        },
      );
      if (!s) {
        setError("当前环境不支持语音识别");
        return;
      }
      session = s;
      setRecording(true);
    })();
  }

  function stop() {
    session?.stop();
    session = null;
    setRecording(false);
  }

  return {
    recording: () => session !== null,
    error: () => "",
    start,
    stop,
    onSpaceDown: (e) => {
      if (e.key !== " " || e.repeat || pttActive) return;
      pttTimer = setTimeout(() => {
        pttActive = true;
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
