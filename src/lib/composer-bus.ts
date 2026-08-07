// composer 跨组件注入通道（Cmd-K 面板 -> TextComposer）。
export const COMPOSER_INSERT_EVENT = "kxen:composer-insert";

export function insertComposerText(text: string): void {
  window.dispatchEvent(new CustomEvent(COMPOSER_INSERT_EVENT, { detail: text }));
}

// 浮层（Cmd-K 面板等）打开时打断 composer 进行中的语音 PTT：
// 焦点被浮层 input 抢走后空格 keyup 落不进 textarea，PTT 永远收不到松开
export const COMPOSER_INTERRUPT_EVENT = "kxen:composer-interrupt";

export function interruptComposer(): void {
  window.dispatchEvent(new CustomEvent(COMPOSER_INTERRUPT_EVENT));
}
