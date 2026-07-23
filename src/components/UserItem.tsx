// user 时间线条目：右对齐 accent 气泡（可选中）+ 悬浮操作（fork / 编辑重发）。
import { Show } from "solid-js";
import MessageActions from "./MessageActions";
import type { MsgItem } from "../lib/items";

export default function UserItem(props: {
  item: MsgItem;
  onFork: () => void;
  onEditResend: (text: string) => void;
}) {
  return (
    <div class="group relative flex flex-col items-end gap-1">
      <div class="selectable max-w-[80%] rounded-2xl rounded-br-md px-3.5 py-2 text-sm bg-[var(--accent)] text-[var(--accent-contrast)] whitespace-pre-wrap">
        {props.item.content}
      </div>
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
