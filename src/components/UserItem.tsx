// user 时间线条目：右对齐 accent 气泡（可选中）+ 图片附件 + 悬浮操作（fork / 编辑重发）。
import { For, Show } from "solid-js";
import MessageActions from "./MessageActions";
import { openMenu } from "../lib/context-menu";
import type { MsgItem } from "../lib/items";

export default function UserItem(props: {
  item: MsgItem;
  onFork: () => void;
  onEditResend: (text: string) => void;
  onRewind: () => void;
}) {
  return (
    <div
      class="group relative flex flex-col items-end gap-1"
      onContextMenu={(e) => {
        openMenu(e, [
          {
            label: "复制内容",
            action: () => void navigator.clipboard.writeText(props.item.content),
          },
          { label: "从此处分叉", action: props.onFork },
          { label: "编辑并重发", action: () => props.onEditResend(props.item.content) },
          { label: "回退到此处", danger: true, action: props.onRewind },
        ]);
      }}
    >
      <Show when={props.item.images?.length}>
        <div class="flex flex-wrap justify-end gap-2">
          <For each={props.item.images}>
            {(img) => (
              <img
                src={`data:${img.media_type};base64,${img.data}`}
                alt="图片附件"
                class="max-h-44 max-w-[60%] rounded-lg border border-[var(--border)] object-contain"
              />
            )}
          </For>
        </div>
      </Show>
      {/* 纯图片消息没有正文，空气泡只是一坨无意义底色 */}
      <Show when={props.item.content}>
        <div class="selectable max-w-[80%] rounded-2xl rounded-br-md px-3.5 py-2 text-sm bg-[var(--accent)] text-[var(--accent-contrast)] whitespace-pre-wrap">
          {props.item.content}
        </div>
      </Show>
      <Show when={props.item.messageId}>
        <div class="self-end">
          <MessageActions
            role="user"
            content={props.item.content}
            onFork={props.onFork}
            onEditResend={props.onEditResend}
          />
        </div>
      </Show>
    </div>
  );
}
