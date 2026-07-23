// textarea 光标视口坐标：镜像 div 复制同排版样式与光标前文本，标记 span 取 rect（标准 mirror 技法）。
// 弹窗锚定唯一用途——textarea 没有 caretFromPoint，只能这样量。

const MIRROR_PROPS = [
  "fontFamily",
  "fontSize",
  "fontWeight",
  "lineHeight",
  "letterSpacing",
  "paddingTop",
  "paddingRight",
  "paddingBottom",
  "paddingLeft",
  "borderTopWidth",
  "borderRightWidth",
  "borderBottomWidth",
  "borderLeftWidth",
  "boxSizing",
] as const;

export function caretRect(textarea: HTMLTextAreaElement): DOMRect | null {
  const cs = getComputedStyle(textarea);
  const taRect = textarea.getBoundingClientRect();
  const div = document.createElement("div");
  Object.assign(div.style, {
    position: "absolute",
    visibility: "hidden",
    whiteSpace: "pre-wrap",
    overflowWrap: "break-word",
    // 镜像必须盖在 textarea 真实位置上，否则量出的是页面原点坐标（弹层飞出屏幕的 bug）
    top: `${taRect.top + window.scrollY}px`,
    left: `${taRect.left + window.scrollX}px`,
  });
  for (const p of MIRROR_PROPS) div.style[p] = cs[p];
  div.style.width = `${textarea.clientWidth}px`;
  div.textContent = textarea.value.slice(0, textarea.selectionStart);
  const marker = document.createElement("span");
  marker.textContent = "​";
  div.appendChild(marker);
  document.body.appendChild(div);
  const rect = marker.getBoundingClientRect();
  div.remove();
  return rect;
}
