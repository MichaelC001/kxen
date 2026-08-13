export const COMMAND_PALETTE_OPEN_EVENT = "kxen:command-palette-open";

/** Sidebar、快捷键等入口共用同一个 Command Palette，不复制搜索状态与数据加载。 */
export function openCommandPalette(): void {
  window.dispatchEvent(new Event(COMMAND_PALETTE_OPEN_EVENT));
}
