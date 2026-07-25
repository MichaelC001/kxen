// 全局快捷键（Cmd/Ctrl）：N 新会话 / W 关当前会话 / , 设置。Layout 挂载一次。
import { flashErr } from "./flash";
import { formatError } from "./error-text";
import { activeSessionId, deleteSession, newSession, navigate } from "./state";

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

/** 关闭当前会话：删除并切到同目录下一条/草稿（善后逻辑收口在 state.deleteSession）。 */
async function closeCurrent(): Promise<void> {
  const id = activeSessionId();
  if (!id) return;
  // 失败只提示不动状态：会话其实还在，activeSessionId 保持原样是对的
  await deleteSession(id).catch((e: unknown) =>
    flashErr(`删除会话失败：${formatError(e instanceof Error ? e.message : String(e))}`),
  );
}
