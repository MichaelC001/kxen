/** 栏宽拖拽把手：pointer capture 跟踪位移，拖拽期间锁光标与文本选中；双击复位默认宽。 */
import { onCleanup } from "solid-js";

export default function ResizeHandle(props: {
  /** 位移增量（px，向右为正）；右栏把手由调用方取反 */
  onDrag: (dx: number) => void;
  onReset: () => void;
  class?: string;
  title?: string;
}) {
  let lastX = 0;
  let dragging = false;

  const end = () => {
    if (!dragging) return;
    dragging = false;
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
  };
  // 拖拽中组件被移除时 pointerup 丢失：兜底还 body 样式，否则 cursor/userSelect 永久残留
  onCleanup(end);

  return (
    <div
      class={`shrink-0 w-1 cursor-col-resize hover:bg-[var(--accent)]/40 transition-colors ${props.class ?? ""}`}
      title={props.title ?? "拖拽调整宽度，双击复位"}
      onPointerDown={(e) => {
        dragging = true;
        lastX = e.clientX;
        e.currentTarget.setPointerCapture(e.pointerId);
        document.body.style.cursor = "col-resize";
        document.body.style.userSelect = "none";
        e.preventDefault();
      }}
      onPointerMove={(e) => {
        if (!dragging) return;
        props.onDrag(e.clientX - lastX);
        lastX = e.clientX;
      }}
      onPointerUp={end}
      onPointerCancel={end}
      onDblClick={() => props.onReset()}
    />
  );
}
