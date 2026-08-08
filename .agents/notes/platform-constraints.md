---
note-type: pitfall
description: Platform constraints
date: 2026-07-24
---

macOS / Windows / Linux 三桌面平台。仅 macOS 的面：voice 麦克风采集（Speech/AVFAudio，objc2 三件套只链 macOS，模块 cfg(target_os = "macos") 排除）、窗口 Overlay/hiddenTitle（main.rs cfg(macos) 代码建窗）、系统编辑菜单（main.rs cfg(macos)）。unix 权限位（0600/0700）一律 `#[cfg(unix)]` 块门控。桌面 bin 窗口/tray/通知是壳能力；业务全在 lib，kxen 无头 bin 复用同一 lib。
