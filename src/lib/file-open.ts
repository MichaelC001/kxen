// 工具行文件路径跳转：桌面端用系统默认应用打开（tauri opener，相对路径按当前会话工作目录解析）；
// web 模式没有本地文件打开能力，复制路径并明说降级，不假装跳转。
import { openPath } from "@tauri-apps/plugin-opener";
import { flashErr, flashOk } from "./flash";
import { formatError } from "./error-text";
import { isTauri } from "./runtime";
import { writeClipboard } from "./clipboard";
import { activeSessionId, sessions } from "./state";

/** 当前会话工作目录（路径解析基准）；无活跃会话/未知 = ""（调用方按相对路径原样处理）。 */
function sessionWorkdir(): string {
  const id = activeSessionId();
  return id ? (sessions().find((s) => s.id === id)?.directory ?? "") : "";
}

export async function openToolPath(path: string): Promise<void> {
  if (!isTauri()) {
    writeClipboard(path);
    flashOk(`已复制路径（Web 模式无法打开本地文件）：${path}`);
    return;
  }
  const workdir = sessionWorkdir();
  const absolute =
    path.startsWith("/") || !workdir ? path : `${workdir.replace(/\/+$/, "")}/${path}`;
  try {
    await openPath(absolute);
  } catch (error) {
    flashErr(`打开文件失败：${formatError(error)}`);
  }
}
