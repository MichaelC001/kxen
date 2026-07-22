// 统一空态行：图标 + 一行灰字（所有空列表共用模式）。
import { Inbox } from "lucide-solid";

export default function EmptyLine(props: { text: string }) {
  return (
    <div class="flex items-center gap-1.5 px-3 py-2.5 text-2xs text-[var(--text-faint)]">
      <Inbox size={11} class="shrink-0" />
      {props.text}
    </div>
  );
}
