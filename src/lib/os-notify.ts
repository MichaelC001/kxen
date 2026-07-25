// OS 桌面通知点击回跳：Rust os_notify 点击通知体后聚焦主窗口并 emit os-notification-click
//（payload = 来源会话 id），本模块是前端唯一接手点。返回注销函数。
import { listen } from "@tauri-apps/api/event";
import { sessions, switchSession } from "./state";
import { flashErr } from "./flash";

export const OS_NOTIFICATION_CLICK = "os-notification-click";

export async function mountOsNotificationJump(): Promise<() => void> {
  return listen<string>(OS_NOTIFICATION_CLICK, (e) => {
    // 与 NotificationCenter.jump 同守卫：通知到达后会话可能已删，悬空切换会让主区变空白
    if (!sessions().some((s) => s.id === e.payload)) {
      flashErr("来源会话已删除");
      return;
    }
    switchSession(e.payload);
  });
}
