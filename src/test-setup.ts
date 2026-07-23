// 测试环境无 Tauri 运行时：主 realm 打桩。
// invoke 黑洞化（永不 settle）：模拟"后端不可用"的稳态——拒绝态在 webkit 下
// 经多层 async 链传递会被 JSC 误报为 unhandled rejection（handler 其实都在），
// 黑洞化后该噪音确定性归零；测试结果不依赖这些 RPC 的返回值。
const w = window as unknown as { __TAURI_INTERNALS__?: Record<string, unknown> };
w.__TAURI_INTERNALS__ = {
  ...w.__TAURI_INTERNALS__,
  invoke: () => new Promise(() => {}),
};
