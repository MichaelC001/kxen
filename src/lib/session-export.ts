import { createEffect, createSignal, type Accessor } from "solid-js";
import { sessionExport } from "./chat";
import { formatError } from "./error-text";

type ExportSession = (sessionId: string) => Promise<{ path: string }>;

export interface SessionExportFlow {
  note: Accessor<string>;
  run: () => Promise<void>;
  dispose: () => void;
}

export function createSessionExport(
  activeSessionId: Accessor<string>,
  exportSession: ExportSession = sessionExport,
): SessionExportFlow {
  const [note, setNote] = createSignal("");
  let sessionGeneration = 0;
  let requestGeneration = 0;
  let clearTimer: ReturnType<typeof setTimeout> | undefined;
  let disposed = false;

  const cancelTimer = () => {
    if (!clearTimer) return;
    clearTimeout(clearTimer);
    clearTimer = undefined;
  };

  createEffect(() => {
    activeSessionId();
    sessionGeneration++;
    requestGeneration++;
    cancelTimer();
    setNote("");
  });

  const run = async () => {
    const sessionId = activeSessionId();
    if (!sessionId || disposed) return;
    const session = sessionGeneration;
    const request = ++requestGeneration;
    cancelTimer();
    setNote("");
    const result = await exportSession(sessionId).then(
      (r) => ({ path: r.path, error: null as unknown }),
      (error: unknown) => ({ path: null as string | null, error }),
    );
    if (
      disposed ||
      session !== sessionGeneration ||
      request !== requestGeneration ||
      activeSessionId() !== sessionId
    ) {
      return;
    }
    // 失败必须带原因（flash 约定）：磁盘满/权限拒绝/会话损坏要可区分
    setNote(result.path ? `已导出 ${result.path}` : `导出失败：${formatError(result.error)}`);
    clearTimer = setTimeout(() => {
      clearTimer = undefined;
      if (!disposed && session === sessionGeneration && request === requestGeneration) setNote("");
    }, 3000);
  };

  const dispose = () => {
    disposed = true;
    requestGeneration++;
    cancelTimer();
  };

  return { note, run, dispose };
}
