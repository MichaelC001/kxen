// composer 跨组件注入通道（Cmd-K 面板 -> TextComposer）。
export const COMPOSER_INSERT_EVENT = "kxen:composer-insert";

export function insertComposerText(text: string): void {
  window.dispatchEvent(new CustomEvent(COMPOSER_INSERT_EVENT, { detail: text }));
}
