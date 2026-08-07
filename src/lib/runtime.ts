// 运行环境判定：Tauri webview 会注入 __TAURI_INTERNALS__，纯浏览器没有。
// 全仓唯一判定入口（env.d.ts 类型声明、test-setup.ts 测试桩是同一字段）；
// 所有 web 模式降级分支一律经这里，不各自摸 window。
export function isTauri(): boolean {
  return typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);
}

export function isWeb(): boolean {
  return !isTauri();
}
