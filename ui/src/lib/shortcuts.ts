// 全局快捷键（Cmd/Ctrl）：N 新会话 / W 关当前会话 / , 设置。Layout 挂载一次。
import { sessionDelete } from "./chat";
import {
  activeSessionId,
  newSession,
  refreshSessions,
  sessions,
  switchSession,
  navigate,
} from "./state";

export function mountShortcuts(): () => void {
  const onKey = (e: KeyboardEvent) => {
    if (!(e.metaKey || e.ctrlKey)) return;
    const key = e.key.toLowerCase();
    if (key === "n") {
      e.preventDefault();
      void newSession();
      return;
    }
    if (key === "w") {
      e.preventDefault();
      void closeCurrent();
      return;
    }
    if (e.key === ",") {
      e.preventDefault();
      navigate("/settings");
    }
  };
  window.addEventListener("keydown", onKey);
  return () => window.removeEventListener("keydown", onKey);
}

/** 关闭当前会话：删除并切到同目录下一条（无则草稿）。 */
async function closeCurrent(): Promise<void> {
  const id = activeSessionId();
  if (!id) return;
  const dir = sessions().find((s) => s.id === id)?.directory;
  await sessionDelete(id).catch(() => {});
  await refreshSessions();
  const next = sessions().find((s) => s.directory === dir) ?? sessions()[0];
  if (next) switchSession(next.id);
  else await newSession();
}
